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
- Twemoji, from git: one 72x72 PNG per emoji.

Two outputs, both committed so a build needs no network:

- qml/js/EmojiData.js, the list the picker reads.
- qml/art/emoji/<codepoints>.png, one per emoji in that list.

    scripts/fetch-emoji.py           fetch, verify, write both
    scripts/fetch-emoji.py --check   fetch, verify, and require the
                                     committed copies to match

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

# emojibase's group numbers, in the order the picker shows them. Group 2
# is the skin-tone and hair components, which are not emoji anyone
# reacts with on their own.
GROUPS = [0, 1, 3, 4, 5, 6, 7, 8, 9]

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA_JS = os.path.join("qml", "js", "EmojiData.js")
ART_DIR = os.path.join("qml", "art", "emoji")


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
    """The 72x72 PNGs, from a checkout that must be the pinned commit."""
    checkout = os.path.join(workdir, "twemoji")
    git = ["git", "-c", "advice.detachedHead=false"]
    subprocess.run(
        git + ["clone", "--quiet", "--depth", "1", "--branch", TWEMOJI_TAG,
               "--filter=blob:none", "--sparse", TWEMOJI_REPO, checkout],
        check=True,
    )
    subprocess.run(git + ["-C", checkout, "sparse-checkout", "set", "assets/72x72"], check=True)
    head = subprocess.run(
        git + ["-C", checkout, "rev-parse", "HEAD"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    if head != TWEMOJI_COMMIT:
        fail("twemoji " + TWEMOJI_TAG + " is " + head + ", not " + TWEMOJI_COMMIT + "; refusing it")
    return os.path.join(checkout, "assets", "72x72")


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
    pictures = {name[:-4] for name in os.listdir(picture_dir) if name.endswith(".png")}
    entries = [e for e in data if e.get("group") in GROUPS and "order" in e]
    entries.sort(key=lambda e: (GROUPS.index(e["group"]), e["order"]))

    rows = []
    used = []
    missing = []
    for entry in entries:
        name = picture_for(entry["emoji"], pictures)
        if name is None:
            missing.append(entry["emoji"])
            continue
        used.append(name)
        rows.append([entry["emoji"], name, GROUPS.index(entry["group"]), search_text(entry)])
    if missing:
        fail("no Twemoji picture for " + " ".join(missing))

    lines = [
        "// Generated by scripts/fetch-emoji.py from emojibase-data "
        + EMOJIBASE_VERSION + " and Twemoji " + TWEMOJI_TAG + ".",
        "// Do not edit: run the script. See vendor/emoji/SOURCE.md.",
        "//",
        "// One entry per emoji the picker offers, in Unicode's order:",
        "// [emoji, picture under qml/art/emoji/ without .png, group, search words].",
        "// Groups: 0 smileys, 1 people, 2 animals and nature, 3 food and drink,",
        "// 4 travel and places, 5 activities, 6 objects, 7 symbols, 8 flags.",
        ".pragma library",
        "",
        "var list = [",
    ]
    for row in rows:
        lines.append("    " + json.dumps(row, ensure_ascii=False) + ",")
    lines.append("]")
    return "\n".join(lines) + "\n", used


def write_tree(out, data_js, used, picture_dir):
    art = os.path.join(out, ART_DIR)
    os.makedirs(art, exist_ok=True)
    os.makedirs(os.path.dirname(os.path.join(out, DATA_JS)), exist_ok=True)
    with open(os.path.join(out, DATA_JS), "w", encoding="utf-8") as handle:
        handle.write(data_js)
    for name in used:
        shutil.copyfile(os.path.join(picture_dir, name + ".png"), os.path.join(art, name + ".png"))


def same_tree(left, right):
    """Whether the generated data and pictures are byte for byte the same,
    with no picture in one that the other lacks."""
    def listing(base):
        art = os.path.join(base, ART_DIR)
        names = sorted(os.listdir(art)) if os.path.isdir(art) else []
        return [DATA_JS] + [os.path.join(ART_DIR, n) for n in names]

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
    if sys.argv[1:] and not check:
        fail("usage: scripts/fetch-emoji.py [--check]")

    with tempfile.TemporaryDirectory() as workdir:
        data = fetch_emojibase()
        picture_dir = fetch_twemoji(workdir)
        data_js, used = build(data, picture_dir)

        if check:
            out = os.path.join(workdir, "out")
            write_tree(out, data_js, used, picture_dir)
            if not same_tree(out, ROOT):
                fail(DATA_JS + " and " + ART_DIR + " are not emojibase-data "
                     + EMOJIBASE_VERSION + " and Twemoji " + TWEMOJI_TAG + "; run scripts/fetch-emoji.py")
            print("fetch-emoji: OK %d emoji match emojibase-data %s and Twemoji %s"
                  % (len(used), EMOJIBASE_VERSION, TWEMOJI_TAG))
            return

        shutil.rmtree(os.path.join(ROOT, ART_DIR), ignore_errors=True)
        write_tree(ROOT, data_js, used, picture_dir)
        print("Done. %d emoji. Remember: vendor/emoji/SOURCE.md must describe these versions." % len(used))


if __name__ == "__main__":
    main()
