/*
 * Engine library's whole interface; src/lib.rs mirrors it field for field and
 * finds these by dlsym(). Only plain types cross: whisper.cpp's structs change
 * between releases.
 */
#ifndef PIIRIT_SPEECH_H
#define PIIRIT_SPEECH_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Results. */
#define PIIRIT_SPEECH_OK 0
#define PIIRIT_SPEECH_CANCELLED 1
/* choose() said not to transcribe. */
#define PIIRIT_SPEECH_STOPPED 2
#define PIIRIT_SPEECH_FAILED (-1)

struct piirit_speech;

/* Called on the running thread. */
struct piirit_speech_hooks {
    void *context;
    /* Once, from first 30 s: probability per language (`n` codes), and that
     * nothing is spoken. Index to transcribe in, or negative to stop. */
    int32_t (*choose)(void *context, const char *const *codes,
                      const float *probs, int32_t n, float no_speech);
    /* 0..=1000. */
    void (*progress)(void *context, int32_t permille);
    /* Text of next segment; valid during call only. */
    void (*segment)(void *context, const char *text);
    /* Nonzero stops; polled before each compute step. */
    int32_t (*cancelled)(void *context);
};

struct piirit_speech_job {
    /* 16 kHz mono. */
    const float *samples;
    int32_t n_samples;
    /* Cores calling thread is pinned to; none = left as is. */
    const int32_t *cpus;
    int32_t n_cpus;
    int32_t threads;
    /* Calling thread to lowest priority, for good. */
    int32_t background;
    /* Encoder positions (1500 = 30 s); 0 = model's. */
    int32_t audio_ctx;
};

/* NULL on failure, reason in `error`. */
struct piirit_speech *piirit_speech_load(const char *model, char *error,
                                         size_t error_len);

void piirit_speech_free(struct piirit_speech *speech);

/* Pins and may lower calling thread for good: call on a thread of its own.
 * Spoken language's code to `language`. */
int piirit_speech_run(struct piirit_speech *speech,
                      const struct piirit_speech_job *job,
                      const struct piirit_speech_hooks *hooks,
                      char *language, size_t language_len,
                      char *error, size_t error_len);

#ifdef __cplusplus
}
#endif

#endif
