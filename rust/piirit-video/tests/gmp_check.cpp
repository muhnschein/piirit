/*
 * Gecko, as far as the H.264 plugin can tell: loads libpiirit.so the way
 * Gecko's plugin process does, hands it a video host and a callback,
 * feeds it pictures, and prints what comes back, one line per event, for
 * tests/gmp_h264.rs to read.
 *
 *     gmp-check <path to libpiirit.so> <scenario>
 *
 * Each scenario is one call's worth of what Gecko does (see scenario()).
 * Every encoded picture is taken apart into the NAL units Gecko's packetizer
 * would read out of it, by the same rules (WebrtcGmpVideoCodec.cpp,
 * WebrtcGmpVideoEncoder::Encoded): a four-byte size in this machine's byte
 * order, then that many bytes, to the end of the buffer exactly.
 *
 * Built by build.rs for the machine running the tests; never packaged.
 */
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include <dlfcn.h>

#include <x264.h>

#include "gmp-entrypoints.h"
#include "gmp-video-encode.h"
#include "gmp-video-host.h"

namespace {

int destroyed_inputs = 0;

class Picture final : public GMPVideoi420Frame {
public:
    Picture(int32_t width, int32_t height, uint64_t timestamp)
        : width_(width), height_(height), timestamp_(timestamp)
    {
        strides_[0] = width + 16;
        strides_[1] = (width + 1) / 2 + 8;
        strides_[2] = strides_[1];
        planes_[0].assign(static_cast<size_t>(strides_[0]) * static_cast<size_t>(height), 0);
        planes_[1].assign(static_cast<size_t>(strides_[1]) * static_cast<size_t>((height + 1) / 2),
                          128);
        planes_[2].assign(planes_[1].size(), 128);
    }

    GMPVideoFrameFormat GetFrameFormat() override { return kGMPI420VideoFrame; }
    void Destroy() override
    {
        destroyed_inputs++;
        delete this;
    }
    GMPErr CreateEmptyFrame(int32_t, int32_t, int32_t, int32_t, int32_t) override
    {
        return GMPNotImplementedErr;
    }
    GMPErr CreateFrame(int32_t, const uint8_t *, int32_t, const uint8_t *, int32_t,
                       const uint8_t *, int32_t, int32_t, int32_t, int32_t,
                       int32_t) override
    {
        return GMPNotImplementedErr;
    }
    GMPErr CopyFrame(const GMPVideoi420Frame &) override { return GMPNotImplementedErr; }
    void SwapFrame(GMPVideoi420Frame *) override {}
    uint8_t *Buffer(GMPPlaneType type) override { return planes_[type].data(); }
    const uint8_t *Buffer(GMPPlaneType type) const override { return planes_[type].data(); }
    int32_t AllocatedSize(GMPPlaneType type) const override
    {
        return static_cast<int32_t>(planes_[type].size());
    }
    int32_t Stride(GMPPlaneType type) const override { return strides_[type]; }
    GMPErr SetWidth(int32_t width) override
    {
        width_ = width;
        return GMPNoErr;
    }
    GMPErr SetHeight(int32_t height) override
    {
        height_ = height;
        return GMPNoErr;
    }
    int32_t Width() const override { return width_; }
    int32_t Height() const override { return height_; }
    void SetTimestamp(uint64_t timestamp) override { timestamp_ = timestamp; }
    uint64_t Timestamp() const override { return timestamp_; }
    void SetDuration(uint64_t) override {}
    uint64_t Duration() const override { return 0; }
    bool IsZeroSize() const override { return width_ == 0 || height_ == 0; }
    void ResetSize() override {}

private:
    int32_t width_;
    int32_t height_;
    uint64_t timestamp_;
    int32_t strides_[3] = {};
    std::vector<uint8_t> planes_[3];
};

class Encoded final : public GMPVideoEncodedFrame {
public:
    GMPVideoFrameFormat GetFrameFormat() override { return kGMPEncodedVideoFrame; }
    void Destroy() override { delete this; }
    GMPErr CreateEmptyFrame(uint32_t size) override
    {
        buffer_.assign(size, 0);
        return GMPNoErr;
    }
    GMPErr CopyFrame(const GMPVideoEncodedFrame &) override { return GMPNotImplementedErr; }
    void SetEncodedWidth(uint32_t width) override { width_ = width; }
    uint32_t EncodedWidth() override { return width_; }
    void SetEncodedHeight(uint32_t height) override { height_ = height; }
    uint32_t EncodedHeight() override { return height_; }
    void SetTimeStamp(uint64_t timestamp) override { timestamp_ = timestamp; }
    uint64_t TimeStamp() override { return timestamp_; }
    void SetDuration(uint64_t) override {}
    uint64_t Duration() const override { return 0; }
    void SetFrameType(GMPVideoFrameType type) override { type_ = type; }
    GMPVideoFrameType FrameType() override { return type_; }
    void SetAllocatedSize(uint32_t) override {}
    uint32_t AllocatedSize() override { return static_cast<uint32_t>(buffer_.size()); }
    void SetSize(uint32_t) override {}
    uint32_t Size() override { return static_cast<uint32_t>(buffer_.size()); }
    void SetCompleteFrame(bool complete) override { complete_ = complete; }
    bool CompleteFrame() override { return complete_; }
    const uint8_t *Buffer() const override { return buffer_.data(); }
    uint8_t *Buffer() override { return buffer_.data(); }
    GMPBufferType BufferType() const override { return buffer_type_; }
    void SetBufferType(GMPBufferType type) override { buffer_type_ = type; }

private:
    std::vector<uint8_t> buffer_;
    uint32_t width_ = 0;
    uint32_t height_ = 0;
    uint64_t timestamp_ = 0;
    GMPVideoFrameType type_ = kGMPVideoFrameInvalid;
    bool complete_ = false;
    GMPBufferType buffer_type_ = GMP_BufferInvalid;
};

class Host final : public GMPVideoHost {
public:
    GMPErr CreateFrame(GMPVideoFrameFormat format, GMPVideoFrame **frame) override
    {
        if (format != kGMPEncodedVideoFrame) {
            return GMPNotImplementedErr;
        }
        *frame = new Encoded();
        return GMPNoErr;
    }
    GMPErr CreatePlane(GMPPlane **) override { return GMPNotImplementedErr; }
};

/* "7:12,8:4,5:3012", or "malformed" when the sizes do not walk the
 * buffer to its end exactly. An SPS also says its profile and level. */
std::string units(const uint8_t *data, uint32_t size, std::string &sps)
{
    std::string out;
    uint32_t at = 0;
    while (at + 4 <= size) {
        uint32_t length = 0;
        std::memcpy(&length, data + at, 4);
        at += 4;
        if (length == 0 || length > size - at) {
            return "malformed";
        }
        int type = data[at] & 0x1f;
        if (type == 7 && length >= 4) {
            char profile[8];
            std::snprintf(profile, sizeof profile, "%02x%02x%02x", data[at + 1],
                          data[at + 2], data[at + 3]);
            sps = profile;
        }
        out += (out.empty() ? "" : ",") + std::to_string(type) + ":" +
               std::to_string(length);
        at += length;
    }
    return at == size ? out : "malformed";
}

class Callback final : public GMPVideoEncoderCallback {
public:
    void Encoded(GMPVideoEncodedFrame *frame, const uint8_t *info,
                 uint32_t info_length) override
    {
        GMPCodecSpecificInfo specific{};
        if (info_length == sizeof specific) {
            std::memcpy(&specific, info, sizeof specific);
        }
        std::string sps = "-";
        std::string nals = units(frame->Buffer(), frame->Size(), sps);
        std::printf("frame ts=%llu type=%s bytes=%u size=%ux%u buffer=%d complete=%d "
                    "info=%d/%d sps=%s nals=%s\n",
                    static_cast<unsigned long long>(frame->TimeStamp()),
                    frame->FrameType() == kGMPKeyFrame     ? "key"
                    : frame->FrameType() == kGMPDeltaFrame ? "delta"
                                                           : "other",
                    frame->Size(), frame->EncodedWidth(), frame->EncodedHeight(),
                    static_cast<int>(frame->BufferType()), frame->CompleteFrame() ? 1 : 0,
                    static_cast<int>(specific.mCodecType),
                    static_cast<int>(specific.mBufferType), sps.c_str(), nals.c_str());
        /* As GMPVideoEncoderChild::Encoded does once it has sent it on. */
        frame->Destroy();
    }

    void Error(GMPErr error) override { std::printf("error %d\n", static_cast<int>(error)); }
};

/* A picture with something in it to encode: a gradient that moves, and
 * a band of noise that is new in every picture, so the bit rate asked
 * for is what limits the size and not the picture. */
Picture *picture(int32_t width, int32_t height, int index, uint64_t timestamp)
{
    auto *frame = new Picture(width, height, timestamp);
    uint8_t *luma = frame->Buffer(kGMPYPlane);
    int32_t stride = frame->Stride(kGMPYPlane);
    uint32_t seed = 2463534242u + static_cast<uint32_t>(index) * 7919u;
    for (int32_t y = 0; y < height; y++) {
        for (int32_t x = 0; x < width; x++) {
            uint8_t value = static_cast<uint8_t>(x + y + index * 4);
            if (y < height / 3) {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                value = static_cast<uint8_t>(seed);
            }
            luma[y * stride + x] = value;
        }
    }
    return frame;
}

struct Session {
    GMPGetAPIFunc get_api;
    Host host;
    Callback callback;
    GMPVideoEncoder *encoder = nullptr;
    int fed = 0;

    bool start(uint32_t width, uint32_t height, uint32_t kbps, uint32_t max_payload)
    {
        void *api = nullptr;
        if (get_api(GMP_API_VIDEO_ENCODER, &host, &api) != GMPNoErr || api == nullptr) {
            std::printf("no encoder\n");
            return false;
        }
        encoder = static_cast<GMPVideoEncoder *>(api);
        /* As WebrtcGmpVideoEncoder::InitEncode fills it: everything else
         * zero, the codec type included. */
        GMPVideoCodec codec;
        std::memset(&codec, 0, sizeof codec);
        codec.mGMPApiVersion = 33;
        codec.mStartBitrate = kbps;
        codec.mMinBitrate = 30;
        codec.mMaxBitrate = 2000;
        codec.mMaxFramerate = 30;
        codec.mMode = kGMPRealtimeVideo;
        codec.mWidth = width;
        codec.mHeight = height;
        encoder->InitEncode(codec, nullptr, 0, &callback, 1, max_payload);
        return true;
    }

    /* `count` pictures, `gap` microseconds apart, a keyframe asked for
     * at `key` (or never, at -1). */
    void feed(int count, int32_t width, int32_t height, uint64_t gap, int key)
    {
        for (int i = 0; i < count; i++, fed++) {
            GMPVideoFrameType type = (i == key) ? kGMPKeyFrame : kGMPDeltaFrame;
            uint64_t timestamp = 1000000 + static_cast<uint64_t>(fed) * gap;
            encoder->Encode(picture(width, height, fed, timestamp), nullptr, 0, &type, 1);
        }
    }

    void finish()
    {
        encoder->EncodingComplete();
        std::printf("inputs destroyed %d of %d\n", destroyed_inputs, fed);
    }
};

constexpr uint64_t FRAME_US = 33333;

#define SYMBOL(name, build) SYMBOL_OF(name, build)
#define SYMBOL_OF(name, build) #name #build

int scenario(const std::string &name, Session &session)
{
    if (name == "call") {
        /* A call: a keyframe to start, one asked for halfway. */
        if (!session.start(320, 240, 300, 0)) {
            return 1;
        }
        session.feed(60, 320, 240, FRAME_US, 30);
    } else if (name == "rates") {
        /* Congestion control turns the rate down to a tenth halfway. */
        if (!session.start(320, 240, 1000, 0)) {
            return 1;
        }
        session.feed(60, 320, 240, FRAME_US, -1);
        session.encoder->SetRates(100, 30);
        session.feed(60, 320, 240, FRAME_US, -1);
    } else if (name == "slow") {
        /* A dim room: the camera gives half the frame rate the call was
         * set up for, and a second must still cost what was asked. */
        if (!session.start(320, 240, 600, 0)) {
            return 1;
        }
        session.feed(90, 320, 240, 2 * FRAME_US, -1);
    } else if (name == "resize") {
        /* The camera's size changes under the same encoder. */
        if (!session.start(320, 240, 300, 0)) {
            return 1;
        }
        session.feed(10, 320, 240, FRAME_US, -1);
        session.feed(10, 160, 120, FRAME_US, -1);
    } else if (name == "odd") {
        if (!session.start(321, 241, 300, 0)) {
            return 1;
        }
        session.feed(5, 321, 241, FRAME_US, -1);
    } else if (name == "slices") {
        /* Packetization mode 0: every unit has to fit one packet. */
        if (!session.start(320, 240, 1000, 400)) {
            return 1;
        }
        session.feed(10, 320, 240, FRAME_US, -1);
    } else if (name == "long") {
        /* A minute of a still picture's pace: one a second. */
        if (!session.start(320, 240, 300, 0)) {
            return 1;
        }
        session.feed(62, 320, 240, 1000000, -1);
    } else if (name == "broken") {
        /* Gecko starting an encoder for a picture of no size. */
        if (!session.start(0, 0, 300, 0)) {
            return 1;
        }
        session.feed(3, 320, 240, FRAME_US, -1);
    } else {
        std::printf("no scenario %s\n", name.c_str());
        return 2;
    }
    session.finish();
    return 0;
}

} // namespace

int main(int argc, char **argv)
{
    if (argc != 3) {
        std::fprintf(stderr, "usage: %s <libpiirit.so> <scenario>\n", argv[0]);
        return 2;
    }
    std::setvbuf(stdout, nullptr, _IOLBF, 0);
    void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (library == nullptr) {
        std::printf("cannot load: %s\n", dlerror());
        return 1;
    }
    auto init = reinterpret_cast<GMPInitFunc>(dlsym(library, "GMPInit"));
    auto get_api = reinterpret_cast<GMPGetAPIFunc>(dlsym(library, "GMPGetAPI"));
    auto shutdown = reinterpret_cast<GMPShutdownFunc>(dlsym(library, "GMPShutdown"));
    if (init == nullptr || get_api == nullptr || shutdown == nullptr) {
        std::printf("missing entry points\n");
        return 1;
    }
    /* x264.h names its functions after its build number. */
    std::printf("x264 symbols %s\n",
                dlsym(library, SYMBOL(x264_encoder_open_, X264_BUILD)) == nullptr
                    ? "hidden"
                    : "visible");
    GMPPlatformAPI platform{};
    if (init(&platform) != GMPNoErr) {
        std::printf("init failed\n");
        return 1;
    }
    Session session{get_api, {}, {}, nullptr, 0};
    void *api = nullptr;
    std::printf("decode-video %d\n", static_cast<int>(get_api("decode-video", &session.host, &api)));
    std::printf("encode-video without host %d\n",
                static_cast<int>(get_api(GMP_API_VIDEO_ENCODER, nullptr, &api)));
    int status = scenario(argv[2], session);
    shutdown();
    return status;
}
