/*
 * Voice file to samples for speech: first sound stream decoded, resampled to
 * mono float at asked rate, handed to sink in chunks.
 *
 * Damaged packets skipped (voice files from many apps); file with no sound
 * stream or no decoder fails.
 */
#include "recode.h"

#include <stdio.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/opt.h>
#include <libswresample/swresample.h>

struct sound_in {
    AVFormatContext *in;
    AVCodecContext *dec;
    struct SwrContext *resample;
    /* Input the resampler is set up for. */
    AVChannelLayout layout;
    int format;
    int rate;
    AVPacket *packet;
    AVFrame *frame;
    float *buffer;
    int capacity;
    int stream;
    int32_t out_rate;
    /* Samples still wanted. */
    int64_t left;
    piirit_samples sink;
    void *context;
    char *error;
    size_t error_len;
};

static void fail(struct sound_in *job, const char *what, int code)
{
    char reason[AV_ERROR_MAX_STRING_SIZE] = {0};
    if (!job->error || job->error_len == 0) {
        return;
    }
    if (code < 0) {
        av_strerror(code, reason, sizeof reason);
        snprintf(job->error, job->error_len, "%s: %s", what, reason);
    } else {
        snprintf(job->error, job->error_len, "%s", what);
    }
}

static void sound_in_free(struct sound_in *job)
{
    swr_free(&job->resample);
    av_channel_layout_uninit(&job->layout);
    avcodec_free_context(&job->dec);
    avformat_close_input(&job->in);
    av_packet_free(&job->packet);
    av_frame_free(&job->frame);
    av_freep(&job->buffer);
}

/* Converts `in` (NULL drains) and hands result on, up to `left`. */
static int put(struct sound_in *job, const AVFrame *in)
{
    int wanted = swr_get_out_samples(job->resample, in ? in->nb_samples : 0);
    int samples;
    if (wanted <= 0) {
        return PIIRIT_OK;
    }
    if (wanted > job->capacity) {
        av_freep(&job->buffer);
        job->buffer = av_malloc_array(wanted, sizeof *job->buffer);
        if (!job->buffer) {
            job->capacity = 0;
            fail(job, "out of memory", 0);
            return PIIRIT_FAILED;
        }
        job->capacity = wanted;
    }
    samples = swr_convert(job->resample, (uint8_t **)&job->buffer, wanted,
                          in ? (const uint8_t **)in->extended_data : NULL,
                          in ? in->nb_samples : 0);
    if (samples < 0) {
        fail(job, "resample the sound", samples);
        return PIIRIT_FAILED;
    }
    if (samples > job->left) {
        samples = (int)job->left;
    }
    if (samples > 0) {
        job->sink(job->context, job->buffer, samples);
        job->left -= samples;
    }
    return PIIRIT_OK;
}

/* Resampler for this frame's format; built again (old one drained) when a
 * stream changes rate or channels midway. */
static int configure(struct sound_in *job, const AVFrame *frame)
{
    AVChannelLayout mono = AV_CHANNEL_LAYOUT_MONO;
    int ret;
    if (job->resample && frame->format == job->format
            && frame->sample_rate == job->rate
            && av_channel_layout_compare(&frame->ch_layout, &job->layout) == 0) {
        return PIIRIT_OK;
    }
    if (job->resample && put(job, NULL) != PIIRIT_OK) {
        return PIIRIT_FAILED;
    }
    swr_free(&job->resample);
    av_channel_layout_uninit(&job->layout);
    ret = av_channel_layout_copy(&job->layout, &frame->ch_layout);
    if (ret >= 0) {
        job->format = frame->format;
        job->rate = frame->sample_rate;
        ret = swr_alloc_set_opts2(&job->resample, &mono, AV_SAMPLE_FMT_FLT,
                                  job->out_rate, &frame->ch_layout,
                                  frame->format, frame->sample_rate, 0, NULL);
    }
    /* Channels averaged: FFmpeg's default stereo-to-mono sum runs past full scale. */
    if (ret >= 0) {
        ret = av_opt_set_double(job->resample, "rematrix_maxval", 1.0, 0);
    }
    if (ret >= 0) {
        ret = swr_init(job->resample);
    }
    if (ret < 0) {
        fail(job, "resample the sound", ret);
        return PIIRIT_FAILED;
    }
    return PIIRIT_OK;
}

static int drain(struct sound_in *job)
{
    int ret = 0;
    while (job->left > 0
            && (ret = avcodec_receive_frame(job->dec, job->frame)) >= 0) {
        ret = configure(job, job->frame);
        if (ret == PIIRIT_OK) {
            ret = put(job, job->frame);
        }
        av_frame_unref(job->frame);
        if (ret != PIIRIT_OK) {
            return ret;
        }
    }
    if (job->left > 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF
            && ret != AVERROR_INVALIDDATA) {
        fail(job, "decode the sound", ret);
        return PIIRIT_FAILED;
    }
    return PIIRIT_OK;
}

static int open_sound(struct sound_in *job, const char *path)
{
    const AVCodec *codec = NULL;
    int ret = avformat_open_input(&job->in, path, NULL, NULL);
    if (ret < 0) {
        fail(job, "open", ret);
        return PIIRIT_FAILED;
    }
    ret = avformat_find_stream_info(job->in, NULL);
    if (ret < 0) {
        fail(job, "read the file", ret);
        return PIIRIT_FAILED;
    }
    job->stream = av_find_best_stream(job->in, AVMEDIA_TYPE_AUDIO, -1, -1,
                                      &codec, 0);
    if (job->stream == AVERROR_DECODER_NOT_FOUND) {
        fail(job, "no decoder for this sound", 0);
        return PIIRIT_FAILED;
    }
    if (job->stream < 0) {
        fail(job, "no sound in the file", 0);
        return PIIRIT_FAILED;
    }
    job->dec = avcodec_alloc_context3(codec);
    if (!job->dec) {
        fail(job, "out of memory", 0);
        return PIIRIT_FAILED;
    }
    ret = avcodec_parameters_to_context(
        job->dec, job->in->streams[job->stream]->codecpar);
    if (ret >= 0) {
        job->dec->pkt_timebase = job->in->streams[job->stream]->time_base;
        ret = avcodec_open2(job->dec, codec, NULL);
    }
    if (ret < 0) {
        fail(job, "open the decoder", ret);
        return PIIRIT_FAILED;
    }
    return PIIRIT_OK;
}

static int read_sound(struct sound_in *job)
{
    int ret = 0;
    while (job->left > 0 && (ret = av_read_frame(job->in, job->packet)) >= 0) {
        if (job->packet->stream_index == job->stream) {
            ret = avcodec_send_packet(job->dec, job->packet);
            if (ret < 0 && ret != AVERROR_INVALIDDATA) {
                av_packet_unref(job->packet);
                fail(job, "decode the sound", ret);
                return PIIRIT_FAILED;
            }
        }
        av_packet_unref(job->packet);
        if (drain(job) != PIIRIT_OK) {
            return PIIRIT_FAILED;
        }
    }
    if (job->left == 0) {
        return PIIRIT_OK;
    }
    if (ret != AVERROR_EOF) {
        fail(job, "read the file", ret);
        return PIIRIT_FAILED;
    }
    ret = avcodec_send_packet(job->dec, NULL);
    if (ret < 0 && ret != AVERROR_EOF) {
        fail(job, "decode the sound", ret);
        return PIIRIT_FAILED;
    }
    if (drain(job) != PIIRIT_OK) {
        return PIIRIT_FAILED;
    }
    return job->resample && job->left > 0 ? put(job, NULL) : PIIRIT_OK;
}

int piirit_video_sound(const char *path, int32_t rate, int64_t max_samples,
                       piirit_samples sink, void *context,
                       char *error, size_t error_len)
{
    struct sound_in job;
    int result = PIIRIT_FAILED;

    av_log_set_level(AV_LOG_QUIET);
    memset(&job, 0, sizeof job);
    job.out_rate = rate;
    job.left = max_samples;
    job.sink = sink;
    job.context = context;
    job.error = error;
    job.error_len = error_len;
    job.packet = av_packet_alloc();
    job.frame = av_frame_alloc();
    if (!job.packet || !job.frame) {
        fail(&job, "out of memory", 0);
    } else if (rate <= 0 || max_samples < 0) {
        fail(&job, "bad rate or length", 0);
    } else if (open_sound(&job, path) == PIIRIT_OK) {
        result = read_sound(&job);
    }
    sound_in_free(&job);
    return result;
}
