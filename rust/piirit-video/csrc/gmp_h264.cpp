/*
 * H.264 encoder for video calls: Gecko Media Plugin around x264.
 *
 * iOS Delta Chat offers and answers H.264 only. Sailfish WebView's Gecko takes
 * H.264 only from a media plugin: this library, as libpiirit.so beside
 * piirit.info, loaded in its own process from `MOZ_GMP_PATH`. Where Jolla's
 * gmp-droid (hardware encoder) is installed, app leaves Gecko to it
 * (piirit-shim/src/call_host.rs).
 *
 * Cheap, as it runs while a call shows picture:
 *  - Constrained Baseline (what Gecko offers, every receiver decodes). x264
 *    "superfast" zerolatency: one ref, no lookahead, no B-frames, one thread,
 *    NEON on ARM. Fastest preset keeping deblocking filter, which is most of
 *    what "ultrafast" loses in picture at call bit rates.
 *  - Picture read in place, not copied.
 *  - Bit rate: Gecko congestion control's ask, 1s buffer; per-second rate raised when camera gives
 *    fewer pictures. Keyframe when far end asks, not on a clock.
 *
 * CPU cost per frame goes to call log when app names one (PIIRIT_CALL_LOG).
 *
 * Methods run on plugin's main thread, one at a time; encoded picture returned
 * inside Encode().
 */
#include <array>
#include <cerrno>
#include <cstdarg>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <new>

#include <fcntl.h>
#include <unistd.h>

#include <x264.h>

#include "gmp-entrypoints.h"
#include "gmp-video-encode.h"
#include "gmp-video-host.h"

#define GMP_EXPORT extern "C" __attribute__((visibility("default")))

namespace {

/* Start rate when Gecko gives none: Gecko's default for new video stream. */
constexpr uint32_t FALLBACK_KBPS = 300;
/* Below this x264 rate control is meaningless at 640x480. */
constexpr uint32_t MIN_KBPS = 30;
constexpr uint32_t FALLBACK_FPS = 30;
/* Floor for spreading rate over fewer pictures: camera off, Gecko sends
 * occasional black pictures, small whatever allowed. */
constexpr uint64_t MIN_FPS = 5;
/* Window the camera fps is measured over. */
constexpr uint64_t PACE_WINDOW_US = 1000000;
/* libwebrtc default keyframe interval, frames. Only a floor for calls where
 * nobody asks: a receiver losing one asks for next. */
constexpr int KEYFRAME_INTERVAL = 3000;
/* Level 3.1: Gecko's H.264 offer (profile-level-id 42e01f). */
constexpr int LEVEL = 31;
/* Cost summary interval, us of picture time (Gecko timestamp unit). */
constexpr uint64_t SUMMARY_EVERY_US = 30000000;
/* 4-byte NAL size before each unit (no Annex B). x264 writes big-endian; Gecko
 * reads this machine's byte order. */
constexpr int LENGTH_BYTES = 4;

/* Call-log line if app named one. File is call host's (made and emptied at call
 * start; not created here), so no call log, nothing written. */
void note(const char *line)
{
    const char *path = std::getenv("PIIRIT_CALL_LOG");
    if (path == nullptr || *path == '\0') {
        return;
    }
    int fd = open(path, O_WRONLY | O_APPEND | O_CLOEXEC);
    if (fd < 0) {
        return;
    }
    /* "encoder" fills column call host writes seconds in. */
    std::array<char, 400> text{};
    if (int length = std::snprintf(text.data(), text.size(), "encoder %s\n", line);
        length > 0) {
        auto size = static_cast<size_t>(length);
        if (size >= text.size()) {
            /* Cut short, still a line. */
            size = text.size() - 1;
            text[size - 1] = '\n';
        }
        ssize_t written;
        do {
            written = write(fd, text.data(), size);
        } while (written < 0 && errno == EINTR);
    }
    close(fd);
}

/* x264 errors only (see open_x264). */
void note_x264(void *, int, const char *format, va_list args)
{
    std::array<char, 300> line{};
    int length = std::vsnprintf(line.data(), line.size(), format, args);
    /* Failed format: nothing; cut line gets last byte as NUL. */
    if (length < 0) {
        length = 0;
    } else if (length >= static_cast<int>(line.size())) {
        length = static_cast<int>(line.size()) - 1;
    }
    if (length > 0 && line[length - 1] == '\n') {
        line[length - 1] = '\0';
    }
    std::array<char, 320> text{};
    std::snprintf(text.data(), text.size(), "x264: %s", line.data());
    note(text.data());
}

/* CPU ns this thread used: battery cost, not wall time. */
int64_t cpu_ns()
{
    timespec now{};
    if (clock_gettime(CLOCK_THREAD_CPUTIME_ID, &now) != 0) {
        return 0;
    }
    return static_cast<int64_t>(now.tv_sec) * 1000000000 + now.tv_nsec;
}

class H264Encoder final : public GMPVideoEncoder {
public:
    explicit H264Encoder(GMPVideoHost *host) : host_(host) {}

    H264Encoder(const H264Encoder &) = delete;
    H264Encoder &operator=(const H264Encoder &) = delete;

    ~H264Encoder() override { close_x264(); }

    void InitEncode(const GMPVideoCodec &settings, const uint8_t *,
                    uint32_t, GMPVideoEncoderCallback *callback, int32_t,
                    uint32_t max_payload) override
    {
        callback_ = callback;
        max_kbps_ = settings.mMaxBitrate;
        kbps_ = held(settings.mStartBitrate == 0 ? FALLBACK_KBPS
                                                 : settings.mStartBitrate);
        fps_ = settings.mMaxFramerate == 0 ? FALLBACK_FPS
                                           : settings.mMaxFramerate;
        /* Gecko: 0 = picture in as many packets as needed (packetization mode 1,
         * usual); else packet size each unit must fit (mode 0). */
        max_slice_ = max_payload;
        if (!open_x264(settings.mWidth, settings.mHeight)) {
            fail("cannot start");
        }
    }

    void Encode(GMPVideoi420Frame *input, const uint8_t *, uint32_t,
                const GMPVideoFrameType *types, uint32_t type_count) override
    {
        bool key = type_count > 0 && types[0] == kGMPKeyFrame;
        if (!failed_) {
            encode(*input, key);
        }
        input->Destroy();
    }

    void SetChannelParameters(uint32_t, uint32_t) override
    {
        /* Loss and round trip already in rate Gecko asks (SetRates). */
    }

    void SetRates(uint32_t kbps, uint32_t) override
    {
        kbps = held(kbps);
        if (kbps == kbps_) {
            return;
        }
        kbps_ = kbps;
        reconfigure();
    }

    void SetPeriodicKeyFrames(bool) override
    {
        /* Gecko never asks; keyframes come when far end asks. */
    }

    void EncodingComplete() override
    {
        summarize("stopped");
        delete this;
    }

private:
    /* Gecko's bit rate, held between x264's floor and call's ceiling. */
    uint32_t held(uint32_t kbps) const
    {
        if (max_kbps_ != 0 && kbps > max_kbps_) {
            kbps = max_kbps_;
        }
        return kbps < MIN_KBPS ? MIN_KBPS : kbps;
    }

    /* Rate per picture at opened fps; fewer pictures get more each, so a second
     * costs what Gecko asked. 1s buffer. */
    void set_rate(x264_param_t &param) const
    {
        uint32_t paced = paced_fps_ == 0 ? fps_ : paced_fps_;
        auto kbps = static_cast<int>(
            paced == 0 ? kbps_ : static_cast<uint64_t>(kbps_) * fps_ / paced);
        param.rc.i_rc_method = X264_RC_ABR;
        param.rc.i_bitrate = kbps;
        param.rc.i_vbv_max_bitrate = kbps;
        param.rc.i_vbv_buffer_size = kbps;
    }

    void reconfigure()
    {
        if (x264_ == nullptr) {
            return;
        }
        x264_param_t param;
        x264_encoder_parameters(x264_, &param);
        set_rate(param);
        x264_encoder_reconfig(x264_, &param);
    }

    /* Camera fps over 1s of timestamps; rate re-set when it moves over 10%. */
    void pace(uint64_t pts)
    {
        if (window_frames_ == 0 || pts < window_pts_) {
            window_pts_ = pts;
            window_frames_ = 1;
            return;
        }
        window_frames_++;
        uint64_t span = pts - window_pts_;
        if (span < PACE_WINDOW_US) {
            return;
        }
        uint64_t fps = ((window_frames_ - 1) * 1000000ULL + span / 2) / span;
        window_pts_ = pts;
        window_frames_ = 1;
        if (fps < MIN_FPS) {
            fps = MIN_FPS;
        }
        if (fps > fps_) {
            fps = fps_;
        }
        uint32_t current = paced_fps_ == 0 ? fps_ : paced_fps_;
        if (uint32_t moved = fps > current ? static_cast<uint32_t>(fps) - current
                                           : current - static_cast<uint32_t>(fps);
            moved * 10 <= current) {
            return;
        }
        paced_fps_ = static_cast<uint32_t>(fps);
        reconfigure();
    }

    bool open_x264(uint32_t width, uint32_t height)
    {
        close_x264();
        /* 4:2:0 needs even sizes; odd column/row dropped. */
        width &= ~1U;
        height &= ~1U;
        if (width == 0 || height == 0) {
            return false;
        }
        x264_param_t param;
        if (x264_param_default_preset(&param, "superfast", "zerolatency") < 0) {
            return false;
        }
        param.i_width = static_cast<int>(width);
        param.i_height = static_cast<int>(height);
        param.i_csp = X264_CSP_I420;
        param.i_threads = 1;
        param.b_sliced_threads = 0;
        param.i_level_idc = LEVEL;
        /* Constant fps, though camera's isn't: driven by timestamps, x264 holds each
         * picture until next says its duration; call can't wait. pace() fixes rate. */
        param.i_fps_num = fps_;
        param.i_fps_den = 1;
        param.i_keyint_max = KEYFRAME_INTERVAL;
        param.i_scenecut_threshold = 0;
        /* SPS and PPS before each keyframe: any receiver can start there. */
        param.b_repeat_headers = 1;
        param.b_annexb = 0;
        param.i_slice_max_size = static_cast<int>(max_slice_);
        param.i_log_level = X264_LOG_ERROR;
        param.pf_log = note_x264;
        set_rate(param);
        if (x264_param_apply_profile(&param, "baseline") < 0) {
            return false;
        }
        x264_ = x264_encoder_open(&param);
        if (x264_ == nullptr) {
            return false;
        }
        width_ = width;
        height_ = height;
        std::array<char, 160> line{};
        std::snprintf(line.data(), line.size(),
                      "x264 %ux%u, up to %u fps, %u kbps, constrained baseline",
                      width_, height_, fps_, kbps_);
        note(line.data());
        return true;
    }

    void close_x264()
    {
        if (x264_ != nullptr) {
            x264_encoder_close(x264_);
            x264_ = nullptr;
        }
    }

    void encode(GMPVideoi420Frame &input, bool key)
    {
        /* Gecko makes new encoder on camera size change, but other-size picture must
         * never reach x264 regardless. */
        if (uint32_t width = static_cast<uint32_t>(input.Width()) & ~1U,
                             height = static_cast<uint32_t>(input.Height()) & ~1U;
            (width != width_ || height != height_) && !open_x264(width, height)) {
            fail("cannot change size");
            return;
        }
        x264_picture_t picture;
        x264_picture_init(&picture);
        picture.img.i_csp = X264_CSP_I420;
        picture.img.i_plane = 3;
        const std::array<GMPPlaneType, 3> planes = {kGMPYPlane, kGMPUPlane,
                                                    kGMPVPlane};
        for (int i = 0; i < 3; i++) {
            picture.img.plane[i] = input.Buffer(planes[i]);
            picture.img.i_stride[i] = input.Stride(planes[i]);
        }
        pace(input.Timestamp());
        picture.i_pts = static_cast<int64_t>(input.Timestamp());
        picture.i_type = key ? X264_TYPE_IDR : X264_TYPE_AUTO;

        x264_nal_t *nals = nullptr;
        int count = 0;
        x264_picture_t out;
        int64_t start = cpu_ns();
        int size = x264_encoder_encode(x264_, &nals, &count, &picture, &out);
        cpu_ns_ += cpu_ns() - start;
        if (size < 0) {
            fail("cannot encode");
            return;
        }
        if (size > 0) {
            deliver(nals, count, out);
        }
    }

    /* Units behind size prefix, as Gecko reads. SEI dropped (x264 version,
     * unread by receivers). */
    void deliver(const x264_nal_t *nals, int count, const x264_picture_t &out)
    {
        uint32_t total = 0;
        for (int i = 0; i < count; i++) {
            if (nals[i].i_type != NAL_SEI) {
                total += static_cast<uint32_t>(nals[i].i_payload);
            }
        }
        GMPVideoFrame *frame = nullptr;
        if (total == 0 || host_->CreateFrame(kGMPEncodedVideoFrame, &frame) != GMPNoErr ||
            frame == nullptr) {
            return;
        }
        auto *encoded = static_cast<GMPVideoEncodedFrame *>(frame);
        if (encoded->CreateEmptyFrame(total) != GMPNoErr) {
            encoded->Destroy();
            return;
        }
        auto *at = reinterpret_cast<std::byte *>(encoded->Buffer());
        for (int i = 0; i < count; i++) {
            if (nals[i].i_type == NAL_SEI) {
                continue;
            }
            auto payload = static_cast<size_t>(nals[i].i_payload);
            std::memcpy(at, nals[i].p_payload, payload);
            auto length = static_cast<uint32_t>(payload - LENGTH_BYTES);
            std::memcpy(at, &length, LENGTH_BYTES);
            at += payload;
        }
        encoded->SetEncodedWidth(width_);
        encoded->SetEncodedHeight(height_);
        encoded->SetTimeStamp(static_cast<uint64_t>(out.i_pts));
        encoded->SetFrameType(out.b_keyframe ? kGMPKeyFrame : kGMPDeltaFrame);
        encoded->SetCompleteFrame(true);
        encoded->SetBufferType(GMP_BufferLength32);

        count_frame(total, out);

        GMPCodecSpecificInfo info{};
        info.mCodecType = kGMPVideoCodecH264;
        info.mBufferType = GMP_BufferLength32;
        /* API takes info as uint8_t bytes: copy struct's bytes, not its address. */
        std::array<uint8_t, sizeof info> info_bytes{};
        std::memcpy(info_bytes.data(), &info, sizeof info);
        callback_->Encoded(encoded, info_bytes.data(), sizeof info);
    }

    void count_frame(uint32_t bytes, const x264_picture_t &out)
    {
        auto pts = static_cast<uint64_t>(out.i_pts);
        frames_++;
        keys_ += out.b_keyframe ? 1 : 0;
        bytes_ += bytes;
        if (frames_ == 1) {
            first_pts_ = pts;
            summary_pts_ = pts;
            std::array<char, 120> line{};
            std::snprintf(line.data(), line.size(),
                          "first picture: %u bytes, %s", bytes,
                          out.b_keyframe ? "key" : "not key");
            note(line.data());
        }
        last_pts_ = pts;
        if (pts - summary_pts_ >= SUMMARY_EVERY_US) {
            summary_pts_ = pts;
            summarize("so far");
        }
    }

    /* "1440 frames (2 key) in 60 s, 4.1 ms CPU each, 290 kbps of 300". */
    void summarize(const char *when) const
    {
        if (frames_ == 0) {
            return;
        }
        double seconds = static_cast<double>(last_pts_ - first_pts_) / 1e6;
        double kbps = seconds > 0 ? static_cast<double>(bytes_) * 8 / seconds / 1000 : 0;
        double ms = static_cast<double>(cpu_ns_) / 1e6 / static_cast<double>(frames_);
        std::array<char, 200> line{};
        std::snprintf(line.data(), line.size(),
                      "%s: %llu frames (%llu key) in %.0f s, %.1f ms CPU each, "
                      "%.0f kbps of %u, %ux%u",
                      when, static_cast<unsigned long long>(frames_),
                      static_cast<unsigned long long>(keys_), seconds, ms, kbps,
                      kbps_, width_, height_);
        note(line.data());
    }

    /* Once told, Gecko sends nothing more; call goes on without picture rather
     * than broken one. */
    void fail(const char *why)
    {
        failed_ = true;
        close_x264();
        std::array<char, 120> line{};
        std::snprintf(line.data(), line.size(), "%s %ux%u at %u kbps", why,
                      width_, height_, kbps_);
        note(line.data());
        if (callback_ != nullptr) {
            callback_->Error(GMPEncodeErr);
        }
    }

    GMPVideoHost *host_;
    GMPVideoEncoderCallback *callback_ = nullptr;
    x264_t *x264_ = nullptr;
    bool failed_ = false;
    uint32_t width_ = 0;
    uint32_t height_ = 0;
    uint32_t fps_ = FALLBACK_FPS;
    uint32_t kbps_ = FALLBACK_KBPS;
    uint32_t max_kbps_ = 0;
    uint32_t max_slice_ = 0;
    uint32_t paced_fps_ = 0;
    uint64_t window_pts_ = 0;
    uint64_t window_frames_ = 0;

    uint64_t frames_ = 0;
    uint64_t keys_ = 0;
    uint64_t bytes_ = 0;
    int64_t cpu_ns_ = 0;
    uint64_t first_pts_ = 0;
    uint64_t last_pts_ = 0;
    uint64_t summary_pts_ = 0;
};

} // namespace

/* Three exports Gecko looks up; rest hidden (x264 stays inside, build.rs). */

GMP_EXPORT GMPErr GMPInit(const GMPPlatformAPI *)
{
    return GMPNoErr;
}

GMP_EXPORT GMPErr GMPGetAPI(const char *name, void *host, void **api)
{
    if (name == nullptr || host == nullptr || api == nullptr ||
        std::strcmp(name, GMP_API_VIDEO_ENCODER) != 0) {
        return GMPNotImplementedErr;
    }
    *api = new (std::nothrow) H264Encoder(static_cast<GMPVideoHost *>(host));
    return *api == nullptr ? GMPAllocErr : GMPNoErr;
}

GMP_EXPORT void GMPShutdown()
{
    /* Nothing held between encoders. */
}
