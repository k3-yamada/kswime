#include "../../include/kswime.h"
#include <stdio.h>
#include <string.h>
int main(void) {
    KswEngine *e = ksw_new();
    ksw_set_context(e, "I bought a ");
    ksw_push(e, "kite");
    ksw_compose(e, 1);
    printf("%s\n", ksw_rendered(e));
    ksw_clear(e); ksw_reset_context(e);
    ksw_push(e, "kaigihazoomdeyarimasu");
    int n = ksw_compose(e, 1);
    printf("%s pieces=%d\n", ksw_rendered(e), n);
    for (int i = 0; i < n; i++)
        printf("  lang=%d %s kana=%s cands=%d\n", ksw_piece_lang(e, i), ksw_piece_text(e, i), ksw_piece_kana(e, i), ksw_piece_candidate_count(e, i));
    ksw_next_candidate(e, -1, 1); ksw_compose(e, 1);
    printf("%s\n", ksw_rendered(e));
    printf("commit: %s  buffer_len=%d\n", ksw_commit(e), ksw_buffer_len(e));
    ksw_push(e, "repo"); ksw_compose(e, 1); printf("%s -> ", ksw_rendered(e));
    ksw_toggle_last_language(e); ksw_compose(e, 1); printf("%s\n", ksw_rendered(e));
    printf("corrections: %s", ksw_export_corrections(e));
    ksw_free(e);
    return 0;
}
