/*
 * The H.264 encoder a video call's picture goes out through, as a Gecko
 * Media Plugin around x264.
 *
 * Delta Chat on iOS offers and answers H.264 and nothing else, and the
 * Sailfish WebView's Gecko encodes H.264 only through a media plugin: a
 * library of its own (this one, built as libpiirit.so beside a
 * piirit.info that says what it does), which Gecko loads in a process of
 * its own from the directories MOZ_GMP_PATH names. The phone's hardware
 * encoder reaches Gecko the same way, through Jolla's gmp-droid, and
 * where that is installed the app leaves Gecko to it
 * (piirit-shim/src/call_host.rs). This is for every phone where it is not.
 *
 * Cheap, because it runs for as long as a call shows a picture:
 *
 *  - Constrained Baseline, which is what Gecko offers and every receiver
 *    decodes, and x264's "superfast" preset tuned for zero latency: one
 *    reference, no lookahead, no B-frames, one thread, NEON on ARM. It is
 *    the fastest preset that keeps the deblocking filter, which is most
 *    of what "ultrafast" gives up in picture at a call's bit rates.
 *  - The picture is read where Gecko put it, not copied first.
 *  - The bit rate is the one Gecko's congestion control asks for, held to
 *    a second's worth of buffer and made good per second when the camera
 *    gives fewer pictures than the call was set up for, and a keyframe is
 *    made when the far end asks for one rather than on a clock.
 *
 * What it costs is written to the call's log when the app names one
 * (PIIRIT_CALL_LOG), in CPU time per frame, so a test call says it.
 *
 * Every method is called on the plugin process's main thread, one at a
 * time, and an encoded picture is handed back from inside Encode().
 */
#include <cerrno>
#include <cstdarg>
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

/* What a call starts at when Gecko does not say: Gecko's own default
 * for a new video stream. */
constexpr uint32_t FALLBACK_KBPS = 300;
/* Below this x264's rate control stops meaning anything at 640x480. */
constexpr uint32_t MIN_KBPS = 30;
constexpr uint32_t FALLBACK_FPS = 30;
/* Below this the rate stops being spread over fewer pictures: while the
 * camera is off Gecko sends a black picture now and then, and those are
 * small whatever they are allowed. */
constexpr uint64_t MIN_FPS = 5;
/* How long the camera's frame rate is measured over. */
constexpr uint64_t PACE_WINDOW_US = 1000000;
/* libwebrtc's own default distance between keyframes, in frames. A
 * receiver that loses one asks for the next, so this is only the floor
 * under a call where nobody ever asks. */
constexpr int KEYFRAME_INTERVAL = 3000;
/* Level 3.1, the level Gecko offers H.264 at (profile-level-id 42e01f). */
constexpr int LEVEL = 31;
/* How often the log is told what encoding costs: in picture time, in
 * microseconds, which is what Gecko's timestamps count. */
constexpr uint64_t SUMMARY_EVERY_US = 30000000;
/* x264 writes four bytes of NAL unit size in front of each unit when it
 * is not asked for Annex B start codes; Gecko reads the same four bytes,
 * in this machine's byte order rather than x264's big-endian. */
constexpr int LENGTH_BYTES = 4;

/* One line in the call's log, when the app has named one. The file is
 * the call host's: it is made, and emptied, when a call starts, so a
 * plugin with no call log to write to writes nothing. */
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
    /* "encoder" fills the column the call host writes its seconds in. */
    char text[400];
    int length = std::snprintf(text, sizeof text, "encoder %s\n", line);
    if (length > 0) {
        size_t size = static_cast<size_t>(length);
        if (size >= sizeof text) {
            /* Cut short, and still a line of its own. */
            size = sizeof text - 1;
            text[size - 1] = '\n';
        }
        ssize_t written;
        do {
            written = write(fd, text, size);
        } while (written < 0 && errno == EINTR);
    }
    close(fd);
}

/* x264's own complaints, which are errors only (see open_x264). */
void note_x264(void *, int, const char *format, va_list args)
{
    char line[300];
    std::vsnprintf(line, sizeof line, format, args);
    size_t length = std::strlen(line);
    if (length > 0 && line[length - 1] == '\n') {
        line[length - 1] = '\0';
    }
    char text[320];
    std::snprintf(text, sizeof text, "x264: %s", line);
    note(text);
}

/* CPU time this thread has used, in nanoseconds: what encoding costs in
 * battery, as opposed to how long it took while other threads ran. */
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
        /* Gecko says 0 when the call packs a picture in as many packets
         * as it needs (packetization mode 1, the usual one), and the
         * packet size when each unit has to fit one (mode 0). */
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

    void SetChannelParameters(uint32_t, uint32_t) override {}

    void SetRates(uint32_t kbps, uint32_t) override
    {
        kbps = held(kbps);
        if (kbps == kbps_) {
            return;
        }
        kbps_ = kbps;
        reconfigure();
    }

    /* Gecko never asks, and keyframes come when the far end asks. */
    void SetPeriodicKeyFrames(bool) override {}

    void EncodingComplete() override
    {
        summarize("stopped");
        delete this;
    }

private:
    /* A bit rate Gecko asks for, held between the floor x264 needs and
     * the ceiling the call was set up with. */
    uint32_t held(uint32_t kbps) const
    {
        if (max_kbps_ != 0 && kbps > max_kbps_) {
            kbps = max_kbps_;
        }
        return kbps < MIN_KBPS ? MIN_KBPS : kbps;
    }

    /* x264 shares the rate out per picture at the frame rate it was
     * opened with; at fewer pictures than that each one is given more,
     * so that a second still costs what Gecko asked for. The buffer
     * holds a second of it. */
    void set_rate(x264_param_t &param) const
    {
        uint32_t paced = paced_fps_ == 0 ? fps_ : paced_fps_;
        int kbps = static_cast<int>(
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

    /* The frame rate the camera actually gives, measured over a second
     * of timestamps, and the rate adjusted to it when it moves by more
     * than a tenth. */
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
        uint64_t fps = ((window_frames_ - 1) * 1000000ull + span / 2) / span;
        window_pts_ = pts;
        window_frames_ = 1;
        if (fps < MIN_FPS) {
            fps = MIN_FPS;
        }
        if (fps > fps_) {
            fps = fps_;
        }
        uint32_t current = paced_fps_ == 0 ? fps_ : paced_fps_;
        uint32_t moved = fps > current ? static_cast<uint32_t>(fps) - current
                                       : current - static_cast<uint32_t>(fps);
        if (moved * 10 <= current) {
            return;
        }
        paced_fps_ = static_cast<uint32_t>(fps);
        reconfigure();
    }

    bool open_x264(uint32_t width, uint32_t height)
    {
        close_x264();
        /* 4:2:0 halves each side for colour, so x264 takes even sizes
         * only; an odd column or row is left off. */
        width &= ~1u;
        height &= ~1u;
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
        /* A constant frame rate, though a camera's is not: x264 holds a
         * picture back until the next one says how long it lasted when
         * told to go by timestamps, and a call cannot spare the wait.
         * pace() makes up the difference in the bit rate. */
        param.i_fps_num = fps_;
        param.i_fps_den = 1;
        param.i_keyint_max = KEYFRAME_INTERVAL;
        param.i_scenecut_threshold = 0;
        /* SPS and PPS before every keyframe, so a receiver can start at
         * any of them -- which is what one that asked for it does. */
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
        char line[160];
        std::snprintf(line, sizeof line,
                      "x264 %ux%u, up to %u fps, %u kbps, constrained baseline",
                      width_, height_, fps_, kbps_);
        note(line);
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
        uint32_t width = static_cast<uint32_t>(input.Width()) & ~1u;
        uint32_t height = static_cast<uint32_t>(input.Height()) & ~1u;
        /* Gecko starts a new encoder when the camera's size changes, but
         * a picture of another size must not reach x264 either way. */
        if ((width != width_ || height != height_) &&
            !open_x264(width, height)) {
            fail("cannot change size");
            return;
        }
        x264_picture_t picture;
        x264_picture_init(&picture);
        picture.img.i_csp = X264_CSP_I420;
        picture.img.i_plane = 3;
        const GMPPlaneType planes[] = {kGMPYPlane, kGMPUPlane, kGMPVPlane};
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

    /* The units of one picture, each behind its size, as Gecko reads
     * them. SEI is left out: x264 writes its version and settings into
     * the first one, which no receiver reads. */
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
        uint8_t *at = encoded->Buffer();
        for (int i = 0; i < count; i++) {
            if (nals[i].i_type == NAL_SEI) {
                continue;
            }
            size_t payload = static_cast<size_t>(nals[i].i_payload);
            std::memcpy(at, nals[i].p_payload, payload);
            uint32_t length = static_cast<uint32_t>(payload - LENGTH_BYTES);
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
        callback_->Encoded(encoded, reinterpret_cast<const uint8_t *>(&info),
                           sizeof info);
    }

    void count_frame(uint32_t bytes, const x264_picture_t &out)
    {
        uint64_t pts = static_cast<uint64_t>(out.i_pts);
        frames_++;
        keys_ += out.b_keyframe ? 1 : 0;
        bytes_ += bytes;
        if (frames_ == 1) {
            first_pts_ = pts;
            summary_pts_ = pts;
            char line[120];
            std::snprintf(line, sizeof line, "first picture: %u bytes, %s",
                          bytes, out.b_keyframe ? "key" : "not key");
            note(line);
        }
        last_pts_ = pts;
        if (pts - summary_pts_ >= SUMMARY_EVERY_US) {
            summary_pts_ = pts;
            summarize("so far");
        }
    }

    /* "1440 frames (2 key) in 60 s, 4.1 ms CPU each, 290 kbps of 300". */
    void summarize(const char *when)
    {
        if (frames_ == 0) {
            return;
        }
        double seconds = static_cast<double>(last_pts_ - first_pts_) / 1e6;
        double kbps = seconds > 0 ? static_cast<double>(bytes_) * 8 / seconds / 1000 : 0;
        double ms = static_cast<double>(cpu_ns_) / 1e6 / static_cast<double>(frames_);
        char line[200];
        std::snprintf(line, sizeof line,
                      "%s: %llu frames (%llu key) in %.0f s, %.1f ms CPU each, "
                      "%.0f kbps of %u, %ux%u",
                      when, static_cast<unsigned long long>(frames_),
                      static_cast<unsigned long long>(keys_), seconds, ms, kbps,
                      kbps_, width_, height_);
        note(line);
    }

    /* Gecko sends nothing more once told; the call goes on without a
     * picture rather than with a broken one. */
    void fail(const char *why)
    {
        failed_ = true;
        close_x264();
        char line[120];
        std::snprintf(line, sizeof line, "%s %ux%u at %u kbps", why, width_,
                      height_, kbps_);
        note(line);
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

/* The three functions Gecko looks the library up by. Nothing else in it
 * is visible: x264's own symbols stay inside (build.rs). */

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
    /* Nothing is held between encoders. */
}
