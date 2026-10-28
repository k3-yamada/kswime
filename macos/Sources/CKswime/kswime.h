/* kswime — C ABI of the dependency-free Japanese/English input engine.
 *
 * All strings are UTF-8 and NUL-terminated.  Strings returned by the engine
 * are owned by the engine and stay valid until the next call on the same
 * engine.  lang: 0 = Japanese, 1 = English, 2 = neutral.  run: index of a
 * Japanese run inside the composition; negative counts from the end.
 * One model is loaded per process (first ksw_new / ksw_preload, ~0.3 s).
 * The engine is not thread-safe; call it from one thread.
 */
#ifndef KSWIME_H
#define KSWIME_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct KswEngine KswEngine;

const char *ksw_version(void);
void        ksw_preload(void);

KswEngine  *ksw_new(void);
void        ksw_free(KswEngine *e);

/* configuration: 0 aggressive, 1 balanced, 2 conservative */
void        ksw_set_strength(KswEngine *e, int32_t strength);
int32_t     ksw_set_param(KswEngine *e, const char *key, double value);

/* context from the host's surrounding text */
void        ksw_set_context(KswEngine *e, const char *before_caret);
void        ksw_set_context_after(KswEngine *e, const char *after_caret);
void        ksw_reset_context(KswEngine *e);

/* keystroke buffer */
void        ksw_push(KswEngine *e, const char *utf8);
int32_t     ksw_backspace(KswEngine *e);   /* 0 when the buffer was empty */
void        ksw_clear(KswEngine *e);
int32_t     ksw_buffer_len(KswEngine *e);
const char *ksw_buffer(KswEngine *e);

/* composition: judge + convert.  final_input=0 while typing, 1 on commit-like
 * keys.  Returns the number of pieces; query them with ksw_piece_*. */
int32_t     ksw_compose(KswEngine *e, int32_t final_input);
const char *ksw_rendered(KswEngine *e);
int32_t     ksw_piece_lang(KswEngine *e, int32_t i);
int32_t     ksw_piece_start(KswEngine *e, int32_t i);
int32_t     ksw_piece_end(KswEngine *e, int32_t i);
int32_t     ksw_piece_pending(KswEngine *e, int32_t i);
int32_t     ksw_piece_confidence(KswEngine *e, int32_t i); /* 0 sure, 1 tentative, 2 undecided */
const char *ksw_piece_text(KswEngine *e, int32_t i);
const char *ksw_piece_kana(KswEngine *e, int32_t i);
int32_t     ksw_piece_candidate_count(KswEngine *e, int32_t i);
int32_t     ksw_piece_selected(KswEngine *e, int32_t i);
const char *ksw_piece_candidate(KswEngine *e, int32_t i, int32_t c);
double      ksw_p_en(KswEngine *e, int32_t keystroke);
int32_t     ksw_stable_prefix(KswEngine *e, double confidence);

/* candidates */
int32_t     ksw_next_candidate(KswEngine *e, int32_t run, int32_t step);
int32_t     ksw_select_candidate(KswEngine *e, int32_t run, int32_t index);

/* learning.  Forcing a language is the user's correction: the per-token
 * memory and the feature weights both move (perceptron step).  The host
 * persists both exports between sessions. */
int32_t     ksw_toggle_last_language(KswEngine *e);  /* new lang, -1 if empty */
void        ksw_force_language(KswEngine *e, int32_t start, int32_t end, int32_t lang);
/* Same, but without learning: for moving a boundary one keystroke at a time.
 * The adjustment is learned once, on ksw_commit or the next ksw_push;
 * ksw_backspace and ksw_clear discard it. */
void        ksw_nudge_language(KswEngine *e, int32_t start, int32_t end, int32_t lang);
void        ksw_feedback(KswEngine *e, const char *token, int32_t lang);
const char *ksw_export_corrections(KswEngine *e);    /* "token\tbias\n"... */
void        ksw_import_corrections(KswEngine *e, const char *text);
const char *ksw_export_weights(KswEngine *e);        /* "NAME\tvalue\n"... */
void        ksw_import_weights(KswEngine *e, const char *text);
void        ksw_set_learning_paused(KswEngine *e, int32_t paused); /* secret mode */
void        ksw_reset_learning(KswEngine *e);        /* defaults; forgets tokens too */
const char *ksw_explain(KswEngine *e);               /* one line per token, "\n"-joined */

/* commit: returns the composed text, clears the buffer, keeps context */
const char *ksw_commit(KswEngine *e);

#ifdef __cplusplus
}
#endif
#endif
