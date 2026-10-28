#!/usr/bin/env bash
# Publish the development repository to the public one.
#
#   tools/publish.sh [--dry-run] [SRC_REF]
#
# Takes the tree of SRC_REF (default: master) from this repository, drops the
# development-only paths listed in EXCLUDE, and pushes it as ONE new commit
# on top of the public repository's main branch (never a force-push; the
# public history is append-only).  The commit message lists the development
# commits since the last publish, which is remembered in the tag
# `published` (pushed to this repository's origin).
#
# One-time setup:
#   git remote add public https://github.com/k3-yamada/kswime.git
set -euo pipefail
cd "$(dirname "$0")/.."

PUBLIC=${PUBLIC_REMOTE:-public}
PUBLIC_BRANCH=${PUBLIC_BRANCH:-main}
DRY=0
SRC=master
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    *) SRC="$a" ;;
  esac
done
EXCLUDE=(.claude .cursor .agents .entire .omc .serena)

git remote get-url "$PUBLIC" >/dev/null 2>&1 || { echo "remote '$PUBLIC' is missing: git remote add public https://github.com/k3-yamada/kswime.git" >&2; exit 1; }
git fetch -q "$PUBLIC" "$PUBLIC_BRANCH"
SRC_SHA=$(git rev-parse "$SRC")
PARENT=$(git rev-parse "$PUBLIC/$PUBLIC_BRANCH")

# Build the published tree in a scratch index so the working tree is untouched.
TMP_INDEX=$(mktemp)
trap 'rm -f "$TMP_INDEX"' EXIT
export GIT_INDEX_FILE="$TMP_INDEX"
git read-tree "$SRC_SHA"
for p in "${EXCLUDE[@]}"; do
  git rm -r -q --cached --ignore-unmatch -- "$p"
done
TREE=$(git write-tree)
unset GIT_INDEX_FILE

if [[ "$(git rev-parse "$PARENT^{tree}")" == "$TREE" ]]; then
  echo "public/$PUBLIC_BRANCH already has this tree ($(git rev-parse --short "$SRC_SHA")); nothing to publish"
  exit 0
fi

LAST=$(git rev-parse -q --verify refs/tags/published 2>/dev/null || true)
RANGE_LOG=$(if [[ -n "$LAST" ]]; then git log --no-merges --format='- %s' "$LAST..$SRC_SHA"; else git log --no-merges -20 --format='- %s' "$SRC_SHA"; fi)
MSG="publish: kswime-dev $(git rev-parse --short "$SRC_SHA") ($(date +%Y-%m-%d))

$RANGE_LOG"

echo "source  $SRC_SHA ($SRC)"
echo "parent  $PARENT ($PUBLIC/$PUBLIC_BRANCH)"
echo "tree    $TREE"
echo "excluded: ${EXCLUDE[*]}"
echo
git diff --stat "$PARENT" "$TREE" | tail -n 20
echo
echo "$MSG" | head -n 25

if [[ $DRY -eq 1 ]]; then
  echo
  echo "(dry run; nothing pushed)"
  exit 0
fi

COMMIT=$(echo "$MSG" | git commit-tree "$TREE" -p "$PARENT")
git push "$PUBLIC" "$COMMIT:refs/heads/$PUBLIC_BRANCH"
git tag -f published "$SRC_SHA" >/dev/null
git push -q -f origin refs/tags/published
echo "published $COMMIT to $PUBLIC/$PUBLIC_BRANCH"
