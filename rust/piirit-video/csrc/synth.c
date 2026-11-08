/*
 * Test clip: moving pattern under noise (costs real bits), tone. As phone
 * camera: H.264 with B-frames, AAC, MP4 with display-matrix turn. Test voice
 * message: tone alone, M4A or ADTS AAC. No fixture.
 */
#include "recode.h"

#include <math.h>
#include <stdio.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/display.h>
#include <x264.h>

#define FPS 30
#define SAMPLE_RATE 48000

struct clip {
    AVFormatContext *out;
    AVStream *video;
    AVStream *audio;
    x264_t *x264;
    x264_picture_t picture;
    int picture_allocated;
    AVCodecContext *aac;
    AVFrame *sound;
    AVPacket *packet;
};

static void clip_free(struct clip *clip)
{
    if (clip->x264) {
        x264_encoder_close(clip->x264);
    }
    if (clip->picture_allocated) {
        x264_picture_clean(&clip->picture);
    }
    avcodec_free_context(&clip->aac);
    av_frame_free(&clip->sound);
    av_packet_free(&clip->packet);
    if (clip->out) {
        if (clip->out->pb) {
            avio_closep(&clip->out->pb);
        }
        avformat_free_context(clip->out);
    }
}

static int put(struct clip *clip, AVRational from, const AVStream *to)
{
    clip->packet->stream_index = to->index;
    av_packet_rescale_ts(clip->packet, from, to->time_base);
    return av_interleaved_write_frame(clip->out, clip->packet);
}

static int put_video(struct clip *clip, x264_picture_t *picture)
{
    x264_picture_t encoded;
    x264_nal_t *nals;
    int count;
    int ret;
    int size = x264_encoder_encode(clip->x264, &nals, &count, picture, &encoded);
    if (size <= 0) {
        return size;
    }
    if (av_new_packet(clip->packet, size) < 0) {
        return -1;
    }
    memcpy(clip->packet->data, nals[0].p_payload, size);
    clip->packet->pts = encoded.i_pts;
    clip->packet->dts = encoded.i_dts;
    if (encoded.b_keyframe) {
        clip->packet->flags |= AV_PKT_FLAG_KEY;
    }
    ret = put(clip, (AVRational){1, FPS}, clip->video);
    av_packet_unref(clip->packet);
    return ret;
}

static int put_audio(struct clip *clip, const AVFrame *frame)
{
    int ret = avcodec_send_frame(clip->aac, frame);
    while (ret >= 0) {
        ret = avcodec_receive_packet(clip->aac, clip->packet);
        if (ret < 0) {
            break;
        }
        ret = put(clip, clip->aac->time_base, clip->audio);
        av_packet_unref(clip->packet);
    }
    return ret == AVERROR(EAGAIN) || ret == AVERROR_EOF ? 0 : ret;
}

static int open_streams(struct clip *clip, const struct piirit_clip *made)
{
    const int32_t width = made->width;
    const int32_t height = made->height;
    x264_param_t param;
    x264_nal_t *nals;
    int count;
    int size = 0;
    uint8_t *extradata;

    /* superfast: keeps B-frames and CABAC, as phones make. medium (no x86 asm)
     * took ~20 frames/s: video tests starved under 32 at once. */
    if (x264_param_default_preset(&param, "superfast", NULL) < 0) {
        return -1;
    }
    param.i_log_level = X264_LOG_NONE;
    param.i_csp = X264_CSP_I420;
    param.i_width = width;
    param.i_height = height;
    param.i_fps_num = FPS;
    param.i_fps_den = 1;
    param.i_timebase_num = 1;
    param.i_timebase_den = FPS;
    param.rc.i_rc_method = X264_RC_ABR;
    param.rc.i_bitrate = (int)(made->bit_rate / 1000);
    param.b_repeat_headers = 0;
    param.b_annexb = 1;
    clip->x264 = x264_encoder_open(&param);
    if (!clip->x264
            || x264_picture_alloc(&clip->picture, X264_CSP_I420, width, height) < 0) {
        return -1;
    }
    clip->picture_allocated = 1;

    clip->video = avformat_new_stream(clip->out, NULL);
    if (!clip->video) {
        return -1;
    }
    clip->video->time_base = (AVRational){1, FPS};
    clip->video->codecpar->codec_type = AVMEDIA_TYPE_VIDEO;
    clip->video->codecpar->codec_id = AV_CODEC_ID_H264;
    clip->video->codecpar->width = width;
    clip->video->codecpar->height = height;
    if (x264_encoder_headers(clip->x264, &nals, &count) < 0) {
        return -1;
    }
    for (int i = 0; i < count; i++) {
        size += nals[i].i_payload;
    }
    extradata = av_mallocz(size + AV_INPUT_BUFFER_PADDING_SIZE);
    if (!extradata) {
        return -1;
    }
    clip->video->codecpar->extradata = extradata;
    clip->video->codecpar->extradata_size = size;
    for (int i = 0; i < count; i++) {
        memcpy(extradata, nals[i].p_payload, nals[i].i_payload);
        extradata += nals[i].i_payload;
    }
    if (made->rotation != 0) {
        AVPacketSideData *matrix = av_packet_side_data_new(
            &clip->video->codecpar->coded_side_data,
            &clip->video->codecpar->nb_coded_side_data,
            AV_PKT_DATA_DISPLAYMATRIX, 9 * sizeof(int32_t), 0);
        if (!matrix) {
            return -1;
        }
        /* Clockwise, as phones mean it (av_display_rotation_set agrees). */
        av_display_rotation_set((int32_t *)matrix->data, made->rotation);
    }

    if (made->with_sound) {
        const AVCodec *codec = avcodec_find_encoder(AV_CODEC_ID_AAC);
        clip->aac = codec ? avcodec_alloc_context3(codec) : NULL;
        if (!clip->aac) {
            return -1;
        }
        clip->aac->sample_fmt = AV_SAMPLE_FMT_FLTP;
        clip->aac->sample_rate = SAMPLE_RATE;
        av_channel_layout_default(&clip->aac->ch_layout, 2);
        /* Phone cameras' sound bit rate. */
        clip->aac->bit_rate = 192000;
        clip->aac->time_base = (AVRational){1, SAMPLE_RATE};
        clip->aac->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
        if (avcodec_open2(clip->aac, codec, NULL) < 0) {
            return -1;
        }
        clip->audio = avformat_new_stream(clip->out, NULL);
        if (!clip->audio
                || avcodec_parameters_from_context(clip->audio->codecpar, clip->aac) < 0) {
            return -1;
        }
        clip->audio->time_base = clip->aac->time_base;
    }
    return 0;
}

/* Moving gradient under non-repeating noise. */
static void draw(const x264_picture_t *picture, int32_t width, int32_t height,
                 int frame, unsigned *seed)
{
    for (int y = 0; y < height; y++) {
        uint8_t *row = picture->img.plane[0] + y * picture->img.i_stride[0];
        for (int x = 0; x < width; x++) {
            *seed = *seed * 1103515245u + 12345u;
            row[x] = (uint8_t)(((x + y + frame * 4) & 0xff) / 2 + 64
                               + ((*seed >> 16) & 0x1f));
        }
    }
    for (int plane = 1; plane < 3; plane++) {
        for (int y = 0; y < height / 2; y++) {
            uint8_t *row = picture->img.plane[plane]
                         + y * picture->img.i_stride[plane];
            for (int x = 0; x < width / 2; x++) {
                row[x] = (uint8_t)(128 + ((x * plane + frame) & 0x3f) - 32);
            }
        }
    }
}

/* One AAC frame of tone from `sample`. */
static int put_tone(struct clip *clip, int64_t sample)
{
    av_frame_unref(clip->sound);
    clip->sound->nb_samples = clip->aac->frame_size;
    clip->sound->format = clip->aac->sample_fmt;
    clip->sound->sample_rate = SAMPLE_RATE;
    av_channel_layout_copy(&clip->sound->ch_layout, &clip->aac->ch_layout);
    if (av_frame_get_buffer(clip->sound, 0) < 0) {
        return -1;
    }
    for (int channel = 0; channel < 2; channel++) {
        float *data = (float *)clip->sound->data[channel];
        for (int i = 0; i < clip->sound->nb_samples; i++) {
            data[i] = 0.3f * (float)sin(2.0 * M_PI * 440.0
                                        * (double)(sample + i) / SAMPLE_RATE);
        }
    }
    clip->sound->pts = sample;
    return put_audio(clip, clip->sound);
}

static int write_clip(struct clip *clip, const char *path,
                      const struct piirit_clip *made)
{
    unsigned seed = 1;
    int64_t sample = 0;

    if (avio_open(&clip->out->pb, path, AVIO_FLAG_WRITE) < 0
            || avformat_write_header(clip->out, NULL) < 0) {
        return -1;
    }
    for (int frame = 0; frame < made->seconds * FPS; frame++) {
        draw(&clip->picture, made->width, made->height, frame, &seed);
        clip->picture.i_pts = frame;
        if (put_video(clip, &clip->picture) < 0) {
            return -1;
        }
        /* Sound up to end of frame. */
        while (clip->aac
                && sample < (int64_t)(frame + 1) * SAMPLE_RATE / FPS) {
            if (put_tone(clip, sample) < 0) {
                return -1;
            }
            sample += clip->sound->nb_samples;
        }
    }
    while (x264_encoder_delayed_frames(clip->x264) > 0) {
        if (put_video(clip, NULL) < 0) {
            return -1;
        }
    }
    if (clip->aac && put_audio(clip, NULL) < 0) {
        return -1;
    }
    return av_write_trailer(clip->out) < 0 ? -1 : 0;
}

int piirit_video_synth(const char *path, const struct piirit_clip *made,
                       char *error, size_t error_len)
{
    struct clip clip;
    int result = PIIRIT_FAILED;

    av_log_set_level(AV_LOG_QUIET);
    memset(&clip, 0, sizeof clip);
    clip.packet = av_packet_alloc();
    clip.sound = av_frame_alloc();
    if (clip.packet && clip.sound
            && avformat_alloc_output_context2(&clip.out, NULL, "mp4", path) >= 0
            && open_streams(&clip, made) == 0
            && write_clip(&clip, path, made) == 0) {
        result = PIIRIT_OK;
    } else if (error && error_len > 0) {
        snprintf(error, error_len, "could not make a %dx%d test clip",
                 (int)made->width, (int)made->height);
    }
    clip_free(&clip);
    return result;
}

/* ADTS header for one raw AAC LC frame of `size` bytes. */
static void adts_header(uint8_t header[7], int rate_index, int channels,
                        int size)
{
    const int length = size + 7;
    header[0] = 0xff;
    /* MPEG-4, layer 0, no CRC. */
    header[1] = 0xf1;
    /* AAC LC: object type 2, stored less one. */
    header[2] = (uint8_t)((1 << 6) | (rate_index << 2) | (channels >> 2));
    header[3] = (uint8_t)(((channels & 3) << 6) | (length >> 11));
    header[4] = (uint8_t)((length >> 3) & 0xff);
    header[5] = (uint8_t)(((length & 7) << 5) | 0x1f);
    header[6] = 0xfc;
}

static int adts_rate_index(int rate)
{
    static const int rates[] = {96000, 88200, 64000, 48000, 44100, 32000,
                                24000, 22050, 16000, 12000, 11025, 8000};
    for (int i = 0; i < (int)(sizeof rates / sizeof rates[0]); i++) {
        if (rates[i] == rate) {
            return i;
        }
    }
    return -1;
}

struct tone {
    AVCodecContext *aac;
    AVFrame *sound;
    AVPacket *packet;
    AVFormatContext *out;
    AVStream *stream;
    FILE *adts;
    int rate_index;
};

static void tone_free(struct tone *tone)
{
    avcodec_free_context(&tone->aac);
    av_frame_free(&tone->sound);
    av_packet_free(&tone->packet);
    if (tone->out) {
        if (tone->out->pb) {
            avio_closep(&tone->out->pb);
        }
        avformat_free_context(tone->out);
    }
    if (tone->adts) {
        fclose(tone->adts);
    }
}

/* Encodes `frame` (NULL drains), writes packets out. */
static int tone_put(struct tone *tone, const AVFrame *frame)
{
    int ret = avcodec_send_frame(tone->aac, frame);
    while (ret >= 0) {
        ret = avcodec_receive_packet(tone->aac, tone->packet);
        if (ret < 0) {
            break;
        }
        if (tone->adts) {
            uint8_t header[7];
            adts_header(header, tone->rate_index,
                        tone->aac->ch_layout.nb_channels, tone->packet->size);
            if (fwrite(header, 1, sizeof header, tone->adts) != sizeof header
                    || fwrite(tone->packet->data, 1, tone->packet->size,
                              tone->adts) != (size_t)tone->packet->size) {
                ret = -1;
            }
        } else {
            tone->packet->stream_index = tone->stream->index;
            av_packet_rescale_ts(tone->packet, tone->aac->time_base,
                                 tone->stream->time_base);
            ret = av_interleaved_write_frame(tone->out, tone->packet);
        }
        av_packet_unref(tone->packet);
    }
    return ret == AVERROR(EAGAIN) || ret == AVERROR_EOF ? 0 : ret;
}

static int tone_open(struct tone *tone, const char *path, int32_t rate,
                     int32_t channels, int32_t adts)
{
    const AVCodec *codec = avcodec_find_encoder(AV_CODEC_ID_AAC);
    tone->aac = codec ? avcodec_alloc_context3(codec) : NULL;
    if (!tone->aac) {
        return -1;
    }
    tone->aac->sample_fmt = AV_SAMPLE_FMT_FLTP;
    tone->aac->sample_rate = rate;
    av_channel_layout_default(&tone->aac->ch_layout, channels);
    tone->aac->bit_rate = 64000;
    tone->aac->time_base = (AVRational){1, rate};
    if (adts) {
        tone->rate_index = adts_rate_index(rate);
        tone->adts = fopen(path, "wb");
        return tone->rate_index < 0 || !tone->adts
            || avcodec_open2(tone->aac, codec, NULL) < 0 ? -1 : 0;
    }
    if (avformat_alloc_output_context2(&tone->out, NULL, "mp4", path) < 0) {
        return -1;
    }
    tone->aac->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    if (avcodec_open2(tone->aac, codec, NULL) < 0) {
        return -1;
    }
    tone->stream = avformat_new_stream(tone->out, NULL);
    if (!tone->stream
            || avcodec_parameters_from_context(tone->stream->codecpar, tone->aac) < 0) {
        return -1;
    }
    tone->stream->time_base = tone->aac->time_base;
    return avio_open(&tone->out->pb, path, AVIO_FLAG_WRITE) < 0
        || avformat_write_header(tone->out, NULL) < 0 ? -1 : 0;
}

static int tone_write(struct tone *tone, int32_t seconds)
{
    const int rate = tone->aac->sample_rate;
    for (int64_t sample = 0; sample < (int64_t)seconds * rate;
         sample += tone->aac->frame_size) {
        av_frame_unref(tone->sound);
        tone->sound->nb_samples = tone->aac->frame_size;
        tone->sound->format = tone->aac->sample_fmt;
        tone->sound->sample_rate = rate;
        av_channel_layout_copy(&tone->sound->ch_layout, &tone->aac->ch_layout);
        if (av_frame_get_buffer(tone->sound, 0) < 0) {
            return -1;
        }
        for (int channel = 0; channel < tone->aac->ch_layout.nb_channels; channel++) {
            float *data = (float *)tone->sound->data[channel];
            for (int i = 0; i < tone->sound->nb_samples; i++) {
                data[i] = 0.3f * (float)sin(2.0 * M_PI * 440.0
                                            * (double)(sample + i) / rate);
            }
        }
        tone->sound->pts = sample;
        if (tone_put(tone, tone->sound) < 0) {
            return -1;
        }
    }
    if (tone_put(tone, NULL) < 0) {
        return -1;
    }
    return tone->out && av_write_trailer(tone->out) < 0 ? -1 : 0;
}

int piirit_video_synth_tone(const char *path, int32_t seconds, int32_t rate,
                            int32_t channels, int32_t adts,
                            char *error, size_t error_len)
{
    struct tone tone;
    int result = PIIRIT_FAILED;

    av_log_set_level(AV_LOG_QUIET);
    memset(&tone, 0, sizeof tone);
    tone.packet = av_packet_alloc();
    tone.sound = av_frame_alloc();
    if (tone.packet && tone.sound
            && tone_open(&tone, path, rate, channels, adts) == 0
            && tone_write(&tone, seconds) == 0) {
        result = PIIRIT_OK;
    } else if (error && error_len > 0) {
        snprintf(error, error_len, "could not make a %d Hz test sound", (int)rate);
    }
    tone_free(&tone);
    return result;
}
