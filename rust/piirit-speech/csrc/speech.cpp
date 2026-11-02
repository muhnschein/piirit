/*
 * whisper.cpp behind speech.h. Built into libpiirit-speech*.so with whisper.cpp
 * and ggml (build.rs); only speech.h's three functions exported.
 *
 * No C++ exception crosses into Rust: every entry point catches.
 */
#include "speech.h"

#include <whisper.h>

#include <sched.h>
#include <sys/resource.h>
#include <sys/syscall.h>
#include <unistd.h>

#include <cmath>
#include <cstdio>
#include <cstring>
#include <exception>
#include <new>
#include <vector>

#define EXPORT extern "C" __attribute__((visibility("default")))

struct piirit_speech {
    whisper_context *context;
};

namespace {

/* Shortest clip whisper.cpp transcribes (it skips under 100 ms). */
constexpr int32_t SHORTEST = WHISPER_SAMPLE_RATE / 10;

/* whisper.cpp's last error line on this thread: the reason behind a NULL. */
thread_local char last_error[256];

void keep_errors(enum ggml_log_level level, const char *text, void *)
{
    if (level != GGML_LOG_LEVEL_ERROR || !text) {
        return;
    }
    snprintf(last_error, sizeof last_error, "%s", text);
    size_t end = strlen(last_error);
    while (end > 0 && (last_error[end - 1] == '\n' || last_error[end - 1] == ' ')) {
        last_error[--end] = '\0';
    }
}

void say(char *error, size_t error_len, const char *what)
{
    if (!error || error_len == 0) {
        return;
    }
    if (last_error[0]) {
        snprintf(error, error_len, "%s (%s)", what, last_error);
    } else {
        snprintf(error, error_len, "%s", what);
    }
}

/* Hooks plus whether a stop was asked: whisper.cpp's error after an abort is
 * just "failed to encode". */
struct run_state {
    const piirit_speech_hooks *hooks;
    bool stopped;
};

bool stop_asked(void *data)
{
    auto *run = static_cast<run_state *>(data);
    if (!run->stopped && run->hooks->cancelled(run->hooks->context) != 0) {
        run->stopped = true;
    }
    return run->stopped;
}

bool encoder_may_begin(whisper_context *, whisper_state *, void *data)
{
    return !stop_asked(data);
}

void on_progress(whisper_context *, whisper_state *, int percent, void *data)
{
    auto *run = static_cast<run_state *>(data);
    /* Past 100 when last window ends beyond the clip. */
    const int32_t permille = percent < 0 ? 0 : percent >= 100 ? 1000 : percent * 10;
    run->hooks->progress(run->hooks->context, permille);
}

void on_segments(whisper_context *, whisper_state *state, int n_new, void *data)
{
    auto *run = static_cast<run_state *>(data);
    const int count = whisper_full_n_segments_from_state(state);
    for (int i = count - n_new; i < count; i++) {
        if (i >= 0) {
            run->hooks->segment(run->hooks->context,
                                whisper_full_get_segment_text_from_state(state, i));
        }
    }
}

/* Calling thread onto `cpus`, threads ggml starts inherit it. Not undone:
 * caller runs this on a thread of its own. Failure (cores offline) leaves it. */
void pin(const int32_t *cpus, int32_t n_cpus)
{
    cpu_set_t set;
    bool any = false;
    CPU_ZERO(&set);
    for (int32_t i = 0; cpus && i < n_cpus; i++) {
        if (cpus[i] >= 0 && cpus[i] < CPU_SETSIZE) {
            CPU_SET(cpus[i], &set);
            any = true;
        }
    }
    if (any) {
        sched_setaffinity(0, sizeof set, &set);
    }
}

/* Nice 19 on this thread only (Linux nice is per thread); unprivileged can't
 * raise it back, hence own thread. */
void lower()
{
    setpriority(PRIO_PROCESS, static_cast<id_t>(syscall(SYS_gettid)), 19);
}

struct state_guard {
    whisper_state *state;
    ~state_guard()
    {
        if (state) {
            whisper_free_state(state);
        }
    }
};

/* Whisper's silence signal: no-speech token's probability right after
 * start-of-transcript, from logits detection's one decode step leaves. 1.9.5's
 * per-segment value reads logits row no step wrote. */
float no_speech(whisper_context *context, whisper_state *state)
{
    const float *logits = whisper_get_logits_from_state(state);
    const int n_vocab = whisper_n_vocab(context);
    const whisper_token nosp = whisper_token_nosp(context);
    if (!logits || n_vocab <= 0 || nosp < 0 || nosp >= n_vocab) {
        return 0.0f;
    }
    float top = logits[0];
    for (int i = 1; i < n_vocab; i++) {
        top = std::fmax(top, logits[i]);
    }
    double sum = 0.0;
    for (int i = 0; i < n_vocab; i++) {
        sum += std::exp(static_cast<double>(logits[i]) - top);
    }
    return static_cast<float>(std::exp(static_cast<double>(logits[nosp]) - top) / sum);
}

/* Detects language and silence; choose() sets `params.language` or STOPPED.
 * Whole 30 s window on purpose (fresh state: no audio_ctx): 0.6 holds there. Trimmed,
 * hum and hiss fall below it and decode to "you". */
int pick_language(whisper_context *context, whisper_state *state,
                  const piirit_speech_job *job, run_state *run,
                  whisper_full_params *params, char *error, size_t error_len)
{
    if (whisper_pcm_to_mel_with_state(context, state, job->samples,
                                      job->n_samples, job->threads) != 0) {
        say(error, error_len, "could not read the sound");
        return PIIRIT_SPEECH_FAILED;
    }
    if (stop_asked(run)) {
        return PIIRIT_SPEECH_CANCELLED;
    }
    const int count = whisper_lang_max_id() + 1;
    std::vector<float> probs(static_cast<size_t>(count), 0.0f);
    std::vector<const char *> codes(static_cast<size_t>(count));
    for (int id = 0; id < count; id++) {
        codes[static_cast<size_t>(id)] = whisper_lang_str(id);
    }
    if (whisper_lang_auto_detect_with_state(context, state, 0, job->threads,
                                            probs.data()) < 0) {
        say(error, error_len, "could not tell the language");
        return PIIRIT_SPEECH_FAILED;
    }
    if (stop_asked(run)) {
        return PIIRIT_SPEECH_CANCELLED;
    }
    const int32_t chosen = run->hooks->choose(run->hooks->context, codes.data(),
                                              probs.data(), count,
                                              no_speech(context, state));
    if (chosen < 0 || chosen >= count) {
        return PIIRIT_SPEECH_STOPPED;
    }
    params->language = codes[static_cast<size_t>(chosen)];
    return PIIRIT_SPEECH_OK;
}

int run(piirit_speech *speech, const piirit_speech_job *job,
        const piirit_speech_hooks *hooks, char *language, size_t language_len,
        char *error, size_t error_len)
{
    run_state run = {hooks, false};
    last_error[0] = '\0';
    if (language && language_len > 0) {
        language[0] = '\0';
    }
    pin(job->cpus, job->n_cpus);
    if (job->background) {
        lower();
    }
    if (job->n_samples < SHORTEST) {
        return PIIRIT_SPEECH_OK;
    }
    state_guard guard = {whisper_init_state(speech->context)};
    if (!guard.state) {
        say(error, error_len, "could not start the engine");
        return PIIRIT_SPEECH_FAILED;
    }

    whisper_full_params params = whisper_full_default_params(WHISPER_SAMPLING_GREEDY);
    params.n_threads = job->threads > 0 ? job->threads : 1;
    params.no_context = true;
    params.no_timestamps = true;
    params.print_special = false;
    params.print_progress = false;
    params.print_realtime = false;
    params.print_timestamps = false;
    params.greedy.best_of = 1;
    params.suppress_blank = true;
    params.suppress_nst = true;
    params.audio_ctx = job->audio_ctx;
    params.progress_callback = on_progress;
    params.progress_callback_user_data = &run;
    params.new_segment_callback = on_segments;
    params.new_segment_callback_user_data = &run;
    params.encoder_begin_callback = encoder_may_begin;
    params.encoder_begin_callback_user_data = &run;
    params.abort_callback = stop_asked;
    params.abort_callback_user_data = &run;

    int result = pick_language(speech->context, guard.state, job, &run, &params,
                               error, error_len);
    if (result != PIIRIT_SPEECH_OK) {
        return result;
    }
    const int code = whisper_full_with_state(speech->context, guard.state, params,
                                             job->samples, job->n_samples);
    if (run.stopped) {
        return PIIRIT_SPEECH_CANCELLED;
    }
    if (code != 0) {
        char what[64];
        snprintf(what, sizeof what, "the engine stopped (%d)", code);
        say(error, error_len, what);
        return PIIRIT_SPEECH_FAILED;
    }
    const char *spoken = whisper_lang_str(whisper_full_lang_id_from_state(guard.state));
    if (language && language_len > 0 && spoken) {
        snprintf(language, language_len, "%s", spoken);
    }
    return PIIRIT_SPEECH_OK;
}

}  // namespace

EXPORT piirit_speech *piirit_speech_load(const char *model, char *error,
                                         size_t error_len)
{
    try {
        last_error[0] = '\0';
        /* No one reads stderr on phone; errors kept for `error`. */
        whisper_log_set(keep_errors, nullptr);
        whisper_context_params params = whisper_context_default_params();
        params.use_gpu = false;
        params.flash_attn = true;
        whisper_context *context =
            whisper_init_from_file_with_params_no_state(model, params);
        if (!context) {
            say(error, error_len, "not a whisper model");
            return nullptr;
        }
        auto *speech = new (std::nothrow) piirit_speech{context};
        if (!speech) {
            whisper_free(context);
            say(error, error_len, "out of memory");
        }
        return speech;
    } catch (const std::exception &failure) {
        say(error, error_len, failure.what());
    } catch (...) {
        say(error, error_len, "the engine failed");
    }
    return nullptr;
}

EXPORT void piirit_speech_free(piirit_speech *speech)
{
    if (speech) {
        whisper_free(speech->context);
        delete speech;
    }
}

EXPORT int piirit_speech_run(piirit_speech *speech,
                             const piirit_speech_job *job,
                             const piirit_speech_hooks *hooks,
                             char *language, size_t language_len,
                             char *error, size_t error_len)
{
    try {
        return run(speech, job, hooks, language, language_len, error, error_len);
    } catch (const std::exception &failure) {
        say(error, error_len, failure.what());
    } catch (...) {
        say(error, error_len, "the engine failed");
    }
    return PIIRIT_SPEECH_FAILED;
}
