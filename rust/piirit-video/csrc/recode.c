/*
 * Make a video smaller: FFmpeg reads and decodes it, scales the picture,
 * x264 encodes it again at the bit rate asked for, and FFmpeg writes the
 * result as an MP4. The sound is encoded again as AAC at its own rate
 * when it is AAC to begin with, and copied as it is otherwise.
 *
 * x264 is driven directly rather than through FFmpeg's libx264 wrapper:
 * that wrapper would tie FFmpeg's configure to finding x264 through
 * pkg-config inside the SDK's scratchbox, and what it does here is a
 * dozen lines below.
 *
 * The picture is scaled as it is stored and not turned: a phone writes
 * its turn into the file as a display matrix, and that matrix is copied
 * to the new file, so it plays the way the old one did.
 */
#include "recode.h"

#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/audio_fifo.h>
#include <libavutil/display.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <libswresample/swresample.h>
#include <libswscale/swscale.h>
#include <x264.h>

/* How x264 trades speed for quality. "veryfast" is the fastest preset
 * that keeps CABAC and the deblocking filter, which are most of what
 * the faster two give up in picture for their speed. */
#define PRESET "veryfast"

static void say(char *error, size_t error_len, const char *format, ...)
{
    va_list args;
    if (!error || error_len == 0) {
        return;
    }
    va_start(args, format);
    vsnprintf(error, error_len, format, args);
    va_end(args);
}

static void say_av(char *error, size_t error_len, const char *what, int code)
{
    char reason[AV_ERROR_MAX_STRING_SIZE] = {0};
    av_strerror(code, reason, sizeof reason);
    say(error, error_len, "%s: %s", what, reason);
}

/* FFmpeg and x264 report on stderr, where nobody on a phone reads it;
 * what went wrong comes back through the error buffer instead. */
static void quiet(void)
{
    av_log_set_level(AV_LOG_QUIET);
}

static int32_t codec_of(enum AVCodecID id)
{
    switch (id) {
    case AV_CODEC_ID_NONE:
        return PIIRIT_CODEC_NONE;
    case AV_CODEC_ID_H264:
        return PIIRIT_CODEC_H264;
    case AV_CODEC_ID_HEVC:
        return PIIRIT_CODEC_HEVC;
    default:
        return PIIRIT_CODEC_OTHER;
    }
}

/* The display matrix's turn, clockwise, to the nearest quarter. */
static int32_t rotation_of(const AVCodecParameters *par)
{
    const AVPacketSideData *matrix = av_packet_side_data_get(
        par->coded_side_data, par->nb_coded_side_data,
        AV_PKT_DATA_DISPLAYMATRIX);
    double counterclockwise;
    long quarters;
    if (!matrix || matrix->size < 9 * sizeof(int32_t)) {
        return 0;
    }
    counterclockwise = av_display_rotation_get((const int32_t *)matrix->data);
    if (isnan(counterclockwise)) {
        return 0;
    }
    quarters = lround(-counterclockwise / 90.0);
    return (int32_t)(((quarters % 4) + 4) % 4) * 90;
}

int piirit_video_probe(const char *path, struct piirit_probe *out,
                       char *error, size_t error_len)
{
    AVFormatContext *format = NULL;
    int video, audio, ret;
    int64_t size;

    quiet();
    memset(out, 0, sizeof *out);
    ret = avformat_open_input(&format, path, NULL, NULL);
    if (ret < 0) {
        say_av(error, error_len, "open", ret);
        return PIIRIT_FAILED;
    }
    /* The header says all of this; nothing is decoded, so asking is
     * cheap enough to do as a file is picked. The file's own duration is
     * only worked out by reading into it, so it is the longest of the
     * streams' instead, which an MP4 header carries. */
    if (format->duration > 0) {
        out->duration_ms = format->duration / (AV_TIME_BASE / 1000);
    }
    for (unsigned i = 0; i < format->nb_streams; i++) {
        const AVStream *stream = format->streams[i];
        if (stream->duration > 0) {
            int64_t ms = av_rescale_q(stream->duration, stream->time_base,
                                      (AVRational){1, 1000});
            if (ms > out->duration_ms) {
                out->duration_ms = ms;
            }
        }
    }
    video = av_find_best_stream(format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    audio = av_find_best_stream(format, AVMEDIA_TYPE_AUDIO, -1, video, NULL, 0);
    if (audio >= 0) {
        const AVCodecParameters *par = format->streams[audio]->codecpar;
        out->audio_codec = par->codec_id == AV_CODEC_ID_AAC ? 1 : 2;
        out->audio_bit_rate = par->bit_rate;
    }
    if (video >= 0) {
        const AVStream *stream = format->streams[video];
        const AVCodecParameters *par = stream->codecpar;
        out->video_codec = codec_of(par->codec_id);
        out->width = par->width;
        out->height = par->height;
        out->rotation = rotation_of(par);
        out->video_bit_rate = par->bit_rate;
        if (out->video_bit_rate <= 0 && out->duration_ms > 0) {
            /* What the file costs per second, less the sound. */
            size = avio_size(format->pb);
            if (size > 0) {
                out->video_bit_rate =
                    size * 8 * 1000 / out->duration_ms - out->audio_bit_rate;
            }
        }
    }
    avformat_close_input(&format);
    return PIIRIT_OK;
}

/* Everything one recoding holds, so that one function can let it go. */
struct job {
    AVFormatContext *in;
    AVFormatContext *out;
    int video_in;
    int audio_in;
    AVStream *video_out;
    AVStream *audio_out;
    AVCodecContext *video_dec;
    AVCodecContext *audio_dec;
    AVCodecContext *audio_enc;
    struct SwsContext *scale;
    struct SwrContext *resample;
    AVAudioFifo *fifo;
    x264_t *x264;
    x264_picture_t picture;
    int picture_allocated;
    AVFrame *frame;
    AVFrame *sound;
    AVPacket *packet;
    const struct piirit_target *target;
    int64_t last_pts;
    int64_t audio_pts;
    int audio_started;
    int64_t start;
    int64_t length;
    int32_t permille;
    piirit_progress progress;
    void *context;
    int cancelled;
    char *error;
    size_t error_len;
};

static void job_free(struct job *job)
{
    if (job->x264) {
        x264_encoder_close(job->x264);
    }
    if (job->picture_allocated) {
        x264_picture_clean(&job->picture);
    }
    sws_freeContext(job->scale);
    swr_free(&job->resample);
    if (job->fifo) {
        av_audio_fifo_free(job->fifo);
    }
    avcodec_free_context(&job->video_dec);
    avcodec_free_context(&job->audio_dec);
    avcodec_free_context(&job->audio_enc);
    av_frame_free(&job->frame);
    av_frame_free(&job->sound);
    av_packet_free(&job->packet);
    avformat_close_input(&job->in);
    if (job->out) {
        if (job->out->pb) {
            avio_closep(&job->out->pb);
        }
        avformat_free_context(job->out);
    }
}

static int open_decoder(struct job *job, int index, AVCodecContext **out)
{
    const AVCodecParameters *par = job->in->streams[index]->codecpar;
    const AVCodec *codec = avcodec_find_decoder(par->codec_id);
    int ret;
    if (!codec) {
        say(job->error, job->error_len, "no decoder for %s",
            avcodec_get_name(par->codec_id));
        return PIIRIT_FAILED;
    }
    *out = avcodec_alloc_context3(codec);
    if (!*out) {
        say(job->error, job->error_len, "out of memory");
        return PIIRIT_FAILED;
    }
    ret = avcodec_parameters_to_context(*out, par);
    if (ret >= 0) {
        (*out)->pkt_timebase = job->in->streams[index]->time_base;
        /* Every core the phone has: decoding is most of the work. */
        (*out)->thread_count = 0;
        (*out)->thread_type = FF_THREAD_FRAME | FF_THREAD_SLICE;
        ret = avcodec_open2(*out, codec, NULL);
    }
    if (ret < 0) {
        say_av(job->error, job->error_len, "open the decoder", ret);
        return PIIRIT_FAILED;
    }
    return PIIRIT_OK;
}

/* x264, set up for the picture asked for, and the stream it writes to. */
static int open_video(struct job *job)
{
    const AVStream *in = job->in->streams[job->video_in];
    AVRational rate = in->avg_frame_rate;
    const AVPacketSideData *matrix;
    x264_param_t param;
    x264_nal_t *nals;
    int count, i, size = 0, kbps;
    uint8_t *extradata;

    if (rate.num <= 0 || rate.den <= 0) {
        rate = in->r_frame_rate;
    }
    if (rate.num <= 0 || rate.den <= 0) {
        rate = (AVRational){30, 1};
    }
    if (x264_param_default_preset(&param, PRESET, NULL) < 0) {
        say(job->error, job->error_len, "x264 has no preset " PRESET);
        return PIIRIT_FAILED;
    }
    kbps = (int)(job->target->video_bit_rate / 1000);
    param.i_log_level = X264_LOG_NONE;
    param.i_threads = X264_THREADS_AUTO;
    param.i_csp = X264_CSP_I420;
    param.i_width = job->target->width;
    param.i_height = job->target->height;
    param.vui.i_sar_width = in->codecpar->sample_aspect_ratio.num;
    param.vui.i_sar_height = in->codecpar->sample_aspect_ratio.den;
    /* The input's own clock, so a phone's variable frame rate survives:
     * x264 is handed each frame's timestamp rather than a frame count. */
    param.b_vfr_input = 1;
    param.i_timebase_num = in->time_base.num;
    param.i_timebase_den = in->time_base.den;
    param.i_fps_num = rate.num;
    param.i_fps_den = rate.den;
    /* An average, held to it closely enough that the file comes out the
     * size it was planned at: what decides whether a relay takes it. */
    param.rc.i_rc_method = X264_RC_ABR;
    param.rc.i_bitrate = kbps;
    param.rc.i_vbv_max_bitrate = kbps * 3 / 2;
    param.rc.i_vbv_buffer_size = kbps * 2;
    /* Headers once, in the file's header, as MP4 has them. */
    param.b_repeat_headers = 0;
    param.b_annexb = 1;
    if (x264_param_apply_profile(&param, "high") < 0) {
        say(job->error, job->error_len, "x264 has no high profile");
        return PIIRIT_FAILED;
    }
    job->x264 = x264_encoder_open(&param);
    if (!job->x264) {
        say(job->error, job->error_len, "x264 would not open");
        return PIIRIT_FAILED;
    }
    if (x264_picture_alloc(&job->picture, X264_CSP_I420, param.i_width,
                           param.i_height) < 0) {
        say(job->error, job->error_len, "out of memory");
        return PIIRIT_FAILED;
    }
    job->picture_allocated = 1;

    job->video_out = avformat_new_stream(job->out, NULL);
    if (!job->video_out) {
        say(job->error, job->error_len, "out of memory");
        return PIIRIT_FAILED;
    }
    job->video_out->time_base = in->time_base;
    job->video_out->avg_frame_rate = rate;
    job->video_out->codecpar->codec_type = AVMEDIA_TYPE_VIDEO;
    job->video_out->codecpar->codec_id = AV_CODEC_ID_H264;
    job->video_out->codecpar->width = param.i_width;
    job->video_out->codecpar->height = param.i_height;
    job->video_out->codecpar->format = AV_PIX_FMT_YUV420P;
    job->video_out->codecpar->bit_rate = job->target->video_bit_rate;
    job->video_out->codecpar->sample_aspect_ratio =
        in->codecpar->sample_aspect_ratio;

    /* The parameter sets, which MP4 keeps in the header rather than in
     * the stream; the muxer turns them into the form it stores. */
    if (x264_encoder_headers(job->x264, &nals, &count) < 0) {
        say(job->error, job->error_len, "x264 wrote no headers");
        return PIIRIT_FAILED;
    }
    for (i = 0; i < count; i++) {
        if (nals[i].i_type == NAL_SPS || nals[i].i_type == NAL_PPS) {
            size += nals[i].i_payload;
        }
    }
    extradata = av_mallocz(size + AV_INPUT_BUFFER_PADDING_SIZE);
    if (!extradata) {
        say(job->error, job->error_len, "out of memory");
        return PIIRIT_FAILED;
    }
    job->video_out->codecpar->extradata = extradata;
    job->video_out->codecpar->extradata_size = size;
    for (i = 0; i < count; i++) {
        if (nals[i].i_type == NAL_SPS || nals[i].i_type == NAL_PPS) {
            memcpy(extradata, nals[i].p_payload, nals[i].i_payload);
            extradata += nals[i].i_payload;
        }
    }

    /* The phone's turn, as the old file had it. */
    matrix = av_packet_side_data_get(in->codecpar->coded_side_data,
                                     in->codecpar->nb_coded_side_data,
                                     AV_PKT_DATA_DISPLAYMATRIX);
    if (matrix) {
        AVPacketSideData *copy = av_packet_side_data_new(
            &job->video_out->codecpar->coded_side_data,
            &job->video_out->codecpar->nb_coded_side_data,
            AV_PKT_DATA_DISPLAYMATRIX, matrix->size, 0);
        if (!copy) {
            say(job->error, job->error_len, "out of memory");
            return PIIRIT_FAILED;
        }
        memcpy(copy->data, matrix->data, matrix->size);
    }
    return open_decoder(job, job->video_in, &job->video_dec);
}

/* The sound: AAC again at the rate asked for, or copied. */
static int open_audio(struct job *job)
{
    const AVStream *in = job->in->streams[job->audio_in];
    const AVCodec *codec;
    int ret;

    job->audio_out = avformat_new_stream(job->out, NULL);
    if (!job->audio_out) {
        say(job->error, job->error_len, "out of memory");
        return PIIRIT_FAILED;
    }
    if (job->target->audio_bit_rate <= 0
            || in->codecpar->codec_id != AV_CODEC_ID_AAC) {
        ret = avcodec_parameters_copy(job->audio_out->codecpar, in->codecpar);
        if (ret < 0) {
            say_av(job->error, job->error_len, "copy the sound", ret);
            return PIIRIT_FAILED;
        }
        job->audio_out->codecpar->codec_tag = 0;
        job->audio_out->time_base = in->time_base;
        return PIIRIT_OK;
    }

    if (open_decoder(job, job->audio_in, &job->audio_dec) != PIIRIT_OK) {
        return PIIRIT_FAILED;
    }
    codec = avcodec_find_encoder(AV_CODEC_ID_AAC);
    job->audio_enc = codec ? avcodec_alloc_context3(codec) : NULL;
    if (!job->audio_enc) {
        say(job->error, job->error_len, "no AAC encoder");
        return PIIRIT_FAILED;
    }
    job->audio_enc->sample_fmt = AV_SAMPLE_FMT_FLTP;
    job->audio_enc->sample_rate =
        job->audio_dec->sample_rate > 0 ? job->audio_dec->sample_rate : 48000;
    /* Stereo at most: a phone's surround track is not what a chat
     * message is for, and every channel costs. */
    if (job->audio_dec->ch_layout.nb_channels == 1) {
        av_channel_layout_default(&job->audio_enc->ch_layout, 1);
    } else {
        av_channel_layout_default(&job->audio_enc->ch_layout, 2);
    }
    job->audio_enc->bit_rate = job->target->audio_bit_rate;
    job->audio_enc->time_base = (AVRational){1, job->audio_enc->sample_rate};
    if (job->out->oformat->flags & AVFMT_GLOBALHEADER) {
        job->audio_enc->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    }
    ret = avcodec_open2(job->audio_enc, codec, NULL);
    if (ret >= 0) {
        ret = avcodec_parameters_from_context(job->audio_out->codecpar,
                                              job->audio_enc);
    }
    if (ret < 0) {
        say_av(job->error, job->error_len, "open the AAC encoder", ret);
        return PIIRIT_FAILED;
    }
    job->audio_out->time_base = job->audio_enc->time_base;
    job->fifo = av_audio_fifo_alloc(job->audio_enc->sample_fmt,
                                    job->audio_enc->ch_layout.nb_channels,
                                    job->audio_enc->frame_size);
    job->sound = av_frame_alloc();
    if (!job->fifo || !job->sound) {
        say(job->error, job->error_len, "out of memory");
        return PIIRIT_FAILED;
    }
    return PIIRIT_OK;
}

static int write_packet(struct job *job, AVPacket *packet, AVRational from,
                        const AVStream *to)
{
    int ret;
    packet->stream_index = to->index;
    av_packet_rescale_ts(packet, from, to->time_base);
    ret = av_interleaved_write_frame(job->out, packet);
    if (ret < 0) {
        say_av(job->error, job->error_len, "write", ret);
        return PIIRIT_FAILED;
    }
    return PIIRIT_OK;
}

/* Hand x264 a picture, or nothing to drain it, and write what comes out. */
static int encode_video(struct job *job, x264_picture_t *picture)
{
    x264_picture_t encoded;
    x264_nal_t *nals;
    int count;
    int size = x264_encoder_encode(job->x264, &nals, &count, picture, &encoded);
    if (size < 0) {
        say(job->error, job->error_len, "x264 could not encode a frame");
        return PIIRIT_FAILED;
    }
    if (size == 0) {
        return PIIRIT_OK;
    }
    if (av_new_packet(job->packet, size) < 0) {
        say(job->error, job->error_len, "out of memory");
        return PIIRIT_FAILED;
    }
    /* x264 lays a frame's units out one after another. */
    memcpy(job->packet->data, nals[0].p_payload, size);
    job->packet->pts = encoded.i_pts;
    job->packet->dts = encoded.i_dts;
    if (encoded.b_keyframe) {
        job->packet->flags |= AV_PKT_FLAG_KEY;
    }
    size = write_packet(job, job->packet,
                        job->in->streams[job->video_in]->time_base,
                        job->video_out);
    av_packet_unref(job->packet);
    return size;
}

/* Say how far along this is, and hear whether to stop. */
static void report(struct job *job, int64_t pts)
{
    int64_t done;
    int32_t permille;
    if (!job->progress || job->length <= 0 || pts == AV_NOPTS_VALUE) {
        return;
    }
    done = (pts - job->start) * 1000 / job->length;
    permille = (int32_t)(done < 0 ? 0 : done > 1000 ? 1000 : done);
    if (permille != job->permille) {
        job->permille = permille;
        if (job->progress(job->context, permille)) {
            job->cancelled = 1;
        }
    }
}

static int scale_and_encode(struct job *job, const AVFrame *frame)
{
    int64_t pts = frame->best_effort_timestamp;
    /* x264 wants every timestamp later than the last. */
    if (pts == AV_NOPTS_VALUE || pts <= job->last_pts) {
        pts = job->last_pts == INT64_MIN ? 0 : job->last_pts + 1;
    }
    job->last_pts = pts;
    job->scale = sws_getCachedContext(
        job->scale, frame->width, frame->height, frame->format,
        job->target->width, job->target->height, AV_PIX_FMT_YUV420P,
        SWS_BICUBIC, NULL, NULL, NULL);
    if (!job->scale) {
        say(job->error, job->error_len, "cannot scale %s",
            av_get_pix_fmt_name(frame->format));
        return PIIRIT_FAILED;
    }
    sws_scale(job->scale, (const uint8_t *const *)frame->data,
              frame->linesize, 0, frame->height, job->picture.img.plane,
              job->picture.img.i_stride);
    job->picture.i_pts = pts;
    job->picture.i_type = X264_TYPE_AUTO;
    report(job, pts);
    return encode_video(job, &job->picture);
}

/* Everything the video decoder has ready. */
static int drain_video(struct job *job)
{
    int ret;
    while ((ret = avcodec_receive_frame(job->video_dec, job->frame)) >= 0) {
        ret = scale_and_encode(job, job->frame);
        av_frame_unref(job->frame);
        if (ret != PIIRIT_OK) {
            return ret;
        }
    }
    if (ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
        say_av(job->error, job->error_len, "decode the video", ret);
        return PIIRIT_FAILED;
    }
    return PIIRIT_OK;
}

static int write_audio_packets(struct job *job)
{
    int ret;
    while ((ret = avcodec_receive_packet(job->audio_enc, job->packet)) >= 0) {
        ret = write_packet(job, job->packet, job->audio_enc->time_base,
                           job->audio_out);
        av_packet_unref(job->packet);
        if (ret != PIIRIT_OK) {
            return ret;
        }
    }
    if (ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
        say_av(job->error, job->error_len, "encode the sound", ret);
        return PIIRIT_FAILED;
    }
    return PIIRIT_OK;
}

/* Hand the encoder whole frames from the queue: all of them when
 * `everything`, which the end of the file asks for. */
static int encode_audio(struct job *job, int everything)
{
    int frame_size = job->audio_enc->frame_size;
    int ret;
    while (av_audio_fifo_size(job->fifo) >= frame_size
            || (everything && av_audio_fifo_size(job->fifo) > 0)) {
        int samples = FFMIN(av_audio_fifo_size(job->fifo), frame_size);
        av_frame_unref(job->sound);
        job->sound->nb_samples = samples;
        job->sound->format = job->audio_enc->sample_fmt;
        job->sound->sample_rate = job->audio_enc->sample_rate;
        ret = av_channel_layout_copy(&job->sound->ch_layout,
                                     &job->audio_enc->ch_layout);
        if (ret >= 0) {
            ret = av_frame_get_buffer(job->sound, 0);
        }
        if (ret < 0) {
            say_av(job->error, job->error_len, "make a sound frame", ret);
            return PIIRIT_FAILED;
        }
        if (av_audio_fifo_read(job->fifo, (void **)job->sound->data, samples)
                < samples) {
            say(job->error, job->error_len, "lost samples");
            return PIIRIT_FAILED;
        }
        job->sound->pts = job->audio_pts;
        job->audio_pts += samples;
        ret = avcodec_send_frame(job->audio_enc, job->sound);
        if (ret < 0) {
            say_av(job->error, job->error_len, "encode the sound", ret);
            return PIIRIT_FAILED;
        }
        if (write_audio_packets(job) != PIIRIT_OK) {
            return PIIRIT_FAILED;
        }
    }
    return PIIRIT_OK;
}

/* Resample whatever came in to what the encoder takes, queue it, and
 * encode what fills a frame. `frame` NULL drains the resampler. */
static int queue_audio(struct job *job, const AVFrame *frame)
{
    uint8_t **buffer = NULL;
    int capacity, samples, ret;

    if (frame && !job->resample) {
        ret = swr_alloc_set_opts2(
            &job->resample, &job->audio_enc->ch_layout,
            job->audio_enc->sample_fmt, job->audio_enc->sample_rate,
            &frame->ch_layout, frame->format, frame->sample_rate, 0, NULL);
        if (ret >= 0) {
            ret = swr_init(job->resample);
        }
        if (ret < 0) {
            say_av(job->error, job->error_len, "resample the sound", ret);
            return PIIRIT_FAILED;
        }
    }
    if (!job->resample) {
        return PIIRIT_OK;
    }
    if (frame && !job->audio_started) {
        /* The sound starts where the old file had it start. */
        job->audio_started = 1;
        if (frame->best_effort_timestamp != AV_NOPTS_VALUE) {
            job->audio_pts = av_rescale_q(
                frame->best_effort_timestamp,
                job->in->streams[job->audio_in]->time_base,
                job->audio_enc->time_base);
        }
    }
    capacity = swr_get_out_samples(job->resample, frame ? frame->nb_samples : 0);
    if (capacity <= 0) {
        return PIIRIT_OK;
    }
    ret = av_samples_alloc_array_and_samples(
        &buffer, NULL, job->audio_enc->ch_layout.nb_channels, capacity,
        job->audio_enc->sample_fmt, 0);
    if (ret < 0) {
        say(job->error, job->error_len, "out of memory");
        return PIIRIT_FAILED;
    }
    samples = swr_convert(job->resample, buffer, capacity,
                          frame ? (const uint8_t **)frame->extended_data : NULL,
                          frame ? frame->nb_samples : 0);
    ret = PIIRIT_OK;
    if (samples < 0) {
        say_av(job->error, job->error_len, "resample the sound", samples);
        ret = PIIRIT_FAILED;
    } else if (samples > 0
            && av_audio_fifo_write(job->fifo, (void **)buffer, samples) < samples) {
        say(job->error, job->error_len, "out of memory");
        ret = PIIRIT_FAILED;
    }
    av_freep(&buffer[0]);
    av_freep(&buffer);
    if (ret != PIIRIT_OK) {
        return ret;
    }
    return encode_audio(job, frame == NULL);
}

static int drain_audio(struct job *job)
{
    int ret;
    while ((ret = avcodec_receive_frame(job->audio_dec, job->frame)) >= 0) {
        ret = queue_audio(job, job->frame);
        av_frame_unref(job->frame);
        if (ret != PIIRIT_OK) {
            return ret;
        }
    }
    if (ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
        say_av(job->error, job->error_len, "decode the sound", ret);
        return PIIRIT_FAILED;
    }
    return PIIRIT_OK;
}

/* One packet off the file, to wherever it goes. */
static int take(struct job *job, AVPacket *packet)
{
    int ret;
    if (packet->stream_index == job->video_in) {
        ret = avcodec_send_packet(job->video_dec, packet);
        if (ret < 0 && ret != AVERROR_INVALIDDATA) {
            say_av(job->error, job->error_len, "decode the video", ret);
            return PIIRIT_FAILED;
        }
        return drain_video(job);
    }
    if (packet->stream_index == job->audio_in) {
        if (!job->audio_dec) {
            return write_packet(job, packet,
                                job->in->streams[job->audio_in]->time_base,
                                job->audio_out);
        }
        ret = avcodec_send_packet(job->audio_dec, packet);
        if (ret < 0 && ret != AVERROR_INVALIDDATA) {
            say_av(job->error, job->error_len, "decode the sound", ret);
            return PIIRIT_FAILED;
        }
        return drain_audio(job);
    }
    return PIIRIT_OK;
}

/* The end of the file: everything still held, out. */
static int finish(struct job *job)
{
    if (avcodec_send_packet(job->video_dec, NULL) < 0
            || drain_video(job) != PIIRIT_OK) {
        return PIIRIT_FAILED;
    }
    while (x264_encoder_delayed_frames(job->x264) > 0) {
        if (encode_video(job, NULL) != PIIRIT_OK) {
            return PIIRIT_FAILED;
        }
    }
    if (job->audio_dec) {
        if (avcodec_send_packet(job->audio_dec, NULL) < 0
                || drain_audio(job) != PIIRIT_OK
                || queue_audio(job, NULL) != PIIRIT_OK) {
            return PIIRIT_FAILED;
        }
        if (avcodec_send_frame(job->audio_enc, NULL) < 0
                || write_audio_packets(job) != PIIRIT_OK) {
            return PIIRIT_FAILED;
        }
    }
    return PIIRIT_OK;
}

static int run(struct job *job, const char *input, const char *output)
{
    const AVStream *video;
    int ret;

    ret = avformat_open_input(&job->in, input, NULL, NULL);
    if (ret < 0) {
        say_av(job->error, job->error_len, "open", ret);
        return PIIRIT_FAILED;
    }
    job->video_in = av_find_best_stream(job->in, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (job->video_in < 0) {
        say(job->error, job->error_len, "no video in the file");
        return PIIRIT_FAILED;
    }
    job->audio_in = av_find_best_stream(job->in, AVMEDIA_TYPE_AUDIO, -1,
                                        job->video_in, NULL, 0);
    video = job->in->streams[job->video_in];
    job->start = video->start_time != AV_NOPTS_VALUE ? video->start_time : 0;
    job->length = video->duration > 0 ? video->duration
                : av_rescale_q(job->in->duration, AV_TIME_BASE_Q, video->time_base);

    ret = avformat_alloc_output_context2(&job->out, NULL, "mp4", output);
    if (ret < 0) {
        say_av(job->error, job->error_len, "start the new file", ret);
        return PIIRIT_FAILED;
    }
    if (open_video(job) != PIIRIT_OK) {
        return PIIRIT_FAILED;
    }
    if (job->audio_in >= 0 && open_audio(job) != PIIRIT_OK) {
        return PIIRIT_FAILED;
    }
    ret = avio_open(&job->out->pb, output, AVIO_FLAG_WRITE);
    if (ret >= 0) {
        ret = avformat_write_header(job->out, NULL);
    }
    if (ret < 0) {
        say_av(job->error, job->error_len, "write the new file", ret);
        return PIIRIT_FAILED;
    }

    while (!job->cancelled && (ret = av_read_frame(job->in, job->packet)) >= 0) {
        ret = take(job, job->packet);
        av_packet_unref(job->packet);
        if (ret != PIIRIT_OK) {
            return ret;
        }
    }
    if (job->cancelled) {
        return PIIRIT_CANCELLED;
    }
    if (ret != AVERROR_EOF) {
        say_av(job->error, job->error_len, "read", ret);
        return PIIRIT_FAILED;
    }
    if (finish(job) != PIIRIT_OK) {
        return PIIRIT_FAILED;
    }
    ret = av_write_trailer(job->out);
    if (ret < 0) {
        say_av(job->error, job->error_len, "finish the new file", ret);
        return PIIRIT_FAILED;
    }
    report(job, job->start + job->length);
    return PIIRIT_OK;
}

int piirit_video_recode(const char *input, const char *output,
                        const struct piirit_target *target,
                        piirit_progress progress, void *context,
                        char *error, size_t error_len)
{
    struct job job;
    int result;

    quiet();
    memset(&job, 0, sizeof job);
    job.target = target;
    job.progress = progress;
    job.context = context;
    job.error = error;
    job.error_len = error_len;
    job.last_pts = INT64_MIN;
    job.permille = -1;
    job.audio_in = -1;
    job.frame = av_frame_alloc();
    job.packet = av_packet_alloc();
    if (!job.frame || !job.packet) {
        say(error, error_len, "out of memory");
        result = PIIRIT_FAILED;
    } else if (target->width <= 0 || target->height <= 0
            || (target->width | target->height) & 1
            || target->video_bit_rate < 1000) {
        say(error, error_len, "cannot make %dx%d at %lld bit/s",
            (int)target->width, (int)target->height,
            (long long)target->video_bit_rate);
        result = PIIRIT_FAILED;
    } else {
        result = run(&job, input, output);
    }
    job_free(&job);
    return result;
}
