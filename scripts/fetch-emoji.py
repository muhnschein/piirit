#!/usr/bin/env python3
"""Fetch the emoji the reaction picker offers, and the pictures it draws them with.

Sailfish cannot draw most emoji itself. Its Qt is 5.6, which has no
colour-font support, and the system fonts cover a few hundred of them in
monochrome at best -- the stock keyboard offers thirty. So the picker,
the quick row and the chips draw Twemoji pictures instead, the way
Fernschreiber does on the same platform, and fall back to text only for
an emoji no picture is shipped for. See vendor/emoji/SOURCE.md.

Two inputs, both pinned:

- emojibase-data, from npm: which emoji there are, in Unicode's order and
  groups, with their CLDR names and keywords for search.
- Twemoji, from git: one SVG per emoji, drawn sharp at any size.

Three outputs, all committed so a build needs no network:

- qml/js/EmojiData.js, the list the picker reads.
- qml/js/EmojiPictures.js, the part of that list drawing an emoji needs,
  as lookup tables built ahead: a message is drawn long before anyone
  opens the picker, and Qt 5.6 parses a script afresh on every launch.
- qml/art/emoji/<codepoints>.svg, one per emoji in that list.

    scripts/fetch-emoji.py             fetch, verify, write them all
    scripts/fetch-emoji.py --check     fetch, verify, and require the
                                       committed copies to match
    scripts/fetch-emoji.py --pictures  write EmojiPictures.js again from
                                       the committed EmojiData.js, offline

Pinned: bump a version *and* its hash together, run this, and update
vendor/emoji/SOURCE.md to match.
"""

import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

EMOJIBASE_VERSION = "17.0.0"
EMOJIBASE_SHA256 = "d01d0e2ca4e22cb402679532155342c71e2e871f25c32223c013b8b166ebdf5a"
EMOJIBASE_URL = (
    "https://registry.npmjs.org/emojibase-data/-/emojibase-data-"
    + EMOJIBASE_VERSION
    + ".tgz"
)

TWEMOJI_TAG = "v17.0.3"
TWEMOJI_COMMIT = "b6b55fef1e8636b540a6d016a4729ca8cdf2e60b"
TWEMOJI_REPO = "https://github.com/jdecked/twemoji.git"

# emojibase group numbers, in picker order. Group 2 (skin-tone and hair
# components) is never reacted with alone.
GROUPS = [0, 1, 3, 4, 5, 6, 7, 8, 9]
# Symbols' index: only group whose text-default chars draw as text without U+FE0F.
SYMBOLS = GROUPS.index(8)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA_JS = os.path.join("qml", "js", "EmojiData.js")
PICTURES_JS = os.path.join("qml", "js", "EmojiPictures.js")
ART_DIR = os.path.join("qml", "art", "emoji")
# Pictures' file type: SVG, sharp at any drawn size.
PICTURE = ".svg"


def fail(message):
    print("fetch-emoji: FAIL " + message, file=sys.stderr)
    sys.exit(1)


def fetch_emojibase():
    """The English data, after checking the tarball is the pinned one."""
    with urllib.request.urlopen(EMOJIBASE_URL) as response:
        blob = response.read()
    if hashlib.sha256(blob).hexdigest() != EMOJIBASE_SHA256:
        fail("checksum mismatch for emojibase-data " + EMOJIBASE_VERSION + "; refusing it")
    with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as tar:
        member = tar.extractfile("package/en/data.json")
        if member is None:
            fail("emojibase-data has no package/en/data.json")
        return json.load(member)


def fetch_twemoji(workdir):
    """The SVGs, from a checkout that must be the pinned commit."""
    checkout = os.path.join(workdir, "twemoji")
    git = ["git", "-c", "advice.detachedHead=false"]
    subprocess.run(
        git + ["clone", "--quiet", "--depth", "1", "--branch", TWEMOJI_TAG,
               "--filter=blob:none", "--sparse", TWEMOJI_REPO, checkout],
        check=True,
    )
    subprocess.run(git + ["-C", checkout, "sparse-checkout", "set", "assets/svg"], check=True)
    head = subprocess.run(
        git + ["-C", checkout, "rev-parse", "HEAD"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    if head != TWEMOJI_COMMIT:
        fail("twemoji " + TWEMOJI_TAG + " is " + head + ", not " + TWEMOJI_COMMIT + "; refusing it")
    return os.path.join(checkout, "assets", "svg")


def picture_for(emoji, pictures):
    """Twemoji's file name for an emoji: its code points in lowercase hex,
    joined by '-', with U+FE0F dropped unless the sequence has a U+200D
    in it. Tried both ways, so a file named the other way is still found."""
    points = ["%x" % ord(c) for c in emoji]
    bare = [p for p in points if p != "fe0f"]
    for name in ("-".join(bare if "200d" not in points else points), "-".join(points), "-".join(bare)):
        if name in pictures:
            return name
    return None


def spelled(entry):
    """An emoji as Unicode spells it, fully qualified: with U+FE0F after a
    lone character only when that character is text unless told otherwise.

    emojibase puts the selector after every lone character, a thumbs-up
    and a star as much as a heart (its `type` says which ones need it:
    0 is text by default, 1 an emoji). Kept, that made a star a symbol
    that is text without the selector -- so a star as everyone sends it,
    bare, was left to Sailfish's font beside the pictures around it -- and
    made the picker react with a thumbs-up other than the quick row's and
    every other client's. Sequences are left as they are: the selectors
    inside them are Unicode's own."""
    emoji = entry["emoji"]
    if entry.get("type") == 1 and len(emoji) == 2 and emoji[1] == "️":
        return emoji[0]
    return emoji


def search_text(entry):
    """What a search matches: the CLDR name and its keywords, lowercase,
    each word once."""
    words = []
    for word in [entry["label"]] + entry.get("tags", []):
        for part in word.lower().split():
            if part not in words:
                words.append(part)
    return " ".join(words)


def build(data, picture_dir):
    pictures = {name[:-4] for name in os.listdir(picture_dir) if name.endswith(PICTURE)}
    entries = [e for e in data if e.get("group") in GROUPS and "order" in e]
    entries.sort(key=lambda e: (GROUPS.index(e["group"]), e["order"]))

    rows = []
    used = []
    missing = []
    for entry in entries:
        emoji = spelled(entry)
        name = picture_for(emoji, pictures)
        if name is None:
            missing.append(emoji)
            continue
        used.append(name)
        rows.append([emoji, name, GROUPS.index(entry["group"]), search_text(entry)])
    if missing:
        fail("no Twemoji picture for " + " ".join(missing))
    return rows, used


def data_js(rows):
    """EmojiData.js: the whole list, search words and all."""
    lines = [
        "// Generated by scripts/fetch-emoji.py from emojibase-data "
        + EMOJIBASE_VERSION + " and Twemoji " + TWEMOJI_TAG + ".",
        "// Do not edit: run the script. See vendor/emoji/SOURCE.md.",
        "//",
        "// One entry per emoji the picker offers, in Unicode's order:",
        "// [emoji, picture under qml/art/emoji/ without " + PICTURE + ", group, search words].",
        "// Groups: 0 smileys, 1 people, 2 animals and nature, 3 food and drink,",
        "// 4 travel and places, 5 activities, 6 objects, 7 symbols, 8 flags.",
        ".pragma library",
        "",
        "var list = [",
    ]
    for row in rows:
        lines.append("    " + json.dumps(row, ensure_ascii=False) + ",")
    lines.append("]")
    return "\n".join(lines) + "\n"


def utf16(text):
    """The UTF-16 code units of `text`, which is what a JS string is."""
    raw = text.encode("utf-16-le")
    return [int.from_bytes(raw[i:i + 2], "little") for i in range(0, len(raw), 2)]


def char_class(units):
    """A regular expression's [...] matching exactly these code units."""
    units = sorted(set(units))
    parts = []
    i = 0
    while i < len(units):
        j = i
        while j + 1 < len(units) and units[j + 1] == units[j] + 1:
            j += 1
        parts.append("\\u%04X" % units[i] if i == j
                     else "\\u%04X-\\u%04X" % (units[i], units[j]))
        i = j + 1
    return "[" + "".join(parts) + "]"


def pictures_js(rows):
    """EmojiPictures.js: what Emoji.js needs to draw an emoji, out of the
    same rows, built here the way Emoji.js used to build it on the phone.
    Emoji.js says what each table is for."""
    by_key = {}
    bmp_starts = []
    text_style = []
    for row in rows:
        emoji = row[0]
        bare = emoji.replace("\ufe0f", "")
        by_key[bare] = row[1]
        first = ord(emoji[0])
        if first <= 0xFFFF:
            if first not in bmp_starts:
                bmp_starts.append(first)
            # One char plus selector: text-style without it. Symbols group
            # only: bare heart, smiley, sun meant as emoji; "(c)", arrow not.
            if (len(bare) == 1 and emoji[1:2] == "\ufe0f" and row[2] == SYMBOLS
                    and bare not in text_style):
                text_style.append(bare)

    # One code unit per key: text without any cannot hold an emoji shipped as a
    # picture. The first (above BMP: surrogate's high half), except keycap: its
    # digit, # or * is in far more text than the keycap mark after it, so mark.
    marks = []
    for bare in by_key:
        units = utf16(bare)
        marks.append(units[0] if units[0] >= 0xA9 or len(units) == 1 else units[1])

    def table(entries):
        return "{\n" + ",\n".join("    " + entry for entry in entries) + "\n}"

    lines = [
        "// Generated by scripts/fetch-emoji.py from emojibase-data "
        + EMOJIBASE_VERSION + " and Twemoji " + TWEMOJI_TAG + ".",
        "// Do not edit: run the script. See vendor/emoji/SOURCE.md.",
        "//",
        "// What drawing an emoji needs out of EmojiData.js, worked out here",
        "// rather than on the phone: messages are drawn on every chat open, the",
        "// picker is opened now and then, and Qt 5.6 parses a script afresh on",
        "// every launch. Emoji.js says what each of these is for.",
        ".pragma library",
        "",
        "var byKey = " + table(json.dumps(key, ensure_ascii=False) + ": " + json.dumps(name)
                               for key, name in by_key.items()),
        "",
        "var bmpStarts = " + table('"%d": true' % code for code in sorted(bmp_starts)),
        "",
        "var textStyle = " + table(json.dumps(bare, ensure_ascii=False) + ": true"
                                   for bare in sorted(text_style)),
        "",
        "var mayHold = /" + char_class(marks) + "/",
    ]
    return "\n".join(lines) + "\n"


def committed_rows():
    """The rows of the committed EmojiData.js, after checking that writing
    them out again gives back the same file."""
    with open(os.path.join(ROOT, DATA_JS), encoding="utf-8") as handle:
        text = handle.read()
    rows = [json.loads(line.strip()[:-1]) for line in text.splitlines()
            if line.startswith("    [") and line.endswith("],")]
    if data_js(rows) != text:
        fail(DATA_JS + " is not in the shape this script writes; run scripts/fetch-emoji.py")
    return rows


def write_scripts(out, rows):
    os.makedirs(os.path.dirname(os.path.join(out, DATA_JS)), exist_ok=True)
    for path, text in ((DATA_JS, data_js(rows)), (PICTURES_JS, pictures_js(rows))):
        with open(os.path.join(out, path), "w", encoding="utf-8") as handle:
            handle.write(text)


def write_tree(out, rows, used, picture_dir):
    art = os.path.join(out, ART_DIR)
    os.makedirs(art, exist_ok=True)
    write_scripts(out, rows)
    for name in used:
        shutil.copyfile(os.path.join(picture_dir, name + PICTURE), os.path.join(art, name + PICTURE))


def same_tree(left, right):
    """Whether the generated data and pictures are byte for byte the same,
    with no picture in one that the other lacks."""
    def listing(base):
        art = os.path.join(base, ART_DIR)
        names = sorted(os.listdir(art)) if os.path.isdir(art) else []
        return [DATA_JS, PICTURES_JS] + [os.path.join(ART_DIR, n) for n in names]

    paths = listing(left)
    if paths != listing(right):
        return False
    for path in paths:
        a, b = os.path.join(left, path), os.path.join(right, path)
        if not os.path.isfile(b):
            return False
        with open(a, "rb") as fa, open(b, "rb") as fb:
            if fa.read() != fb.read():
                return False
    return True


def main():
    check = sys.argv[1:] == ["--check"]
    if sys.argv[1:] == ["--pictures"]:
        write_scripts(ROOT, committed_rows())
        print("Done. " + PICTURES_JS + " written from " + DATA_JS + ".")
        return
    if sys.argv[1:] and not check:
        fail("usage: scripts/fetch-emoji.py [--check | --pictures]")

    with tempfile.TemporaryDirectory() as workdir:
        data = fetch_emojibase()
        picture_dir = fetch_twemoji(workdir)
        rows, used = build(data, picture_dir)

        if check:
            out = os.path.join(workdir, "out")
            write_tree(out, rows, used, picture_dir)
            if not same_tree(out, ROOT):
                fail(DATA_JS + ", " + PICTURES_JS + " and " + ART_DIR + " are not emojibase-data "
                     + EMOJIBASE_VERSION + " and Twemoji " + TWEMOJI_TAG + "; run scripts/fetch-emoji.py")
            print("fetch-emoji: OK %d emoji match emojibase-data %s and Twemoji %s"
                  % (len(used), EMOJIBASE_VERSION, TWEMOJI_TAG))
            return

        shutil.rmtree(os.path.join(ROOT, ART_DIR), ignore_errors=True)
        write_tree(ROOT, rows, used, picture_dir)
        print("Done. %d emoji. Remember: vendor/emoji/SOURCE.md must describe these versions." % len(used))


if __name__ == "__main__":
    main()
