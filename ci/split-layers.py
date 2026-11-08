#!/usr/bin/env python3
"""Split a flattened rootfs tar into N tars of about equal size.

    ci/split-layers.py <rootfs.tar> <outdir> <n>

Writes <outdir>/layer-<i>.tar; build-sdk-image.sh ADDs each as own layer.
One 2 GB layer downloads as one stream (ghcr: 0:29 to 3:49 per pull);
several pull in parallel. Members copied as they are: hard link into an
earlier tar resolves against lower layer on ADD.
"""
import os
import sys
import tarfile


def split(src, outdir, n):
    with tarfile.open(src, "r:") as tar:
        members = tar.getmembers()
        share = sum(m.size for m in members if m.isreg()) / n
        index, written, out = 0, 0, None
        for member in members:
            if out is None or (written >= share * (index + 1) and index < n - 1):
                if out is not None:
                    out.close()
                    index += 1
                out = tarfile.open(
                    os.path.join(outdir, f"layer-{index}.tar"), "w",
                    format=tarfile.PAX_FORMAT)
            data = tar.extractfile(member) if member.isreg() else None
            written += member.size if member.isreg() else 0
            out.addfile(member, data)
        out.close()
    return index + 1


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__.split("\n\n")[1])
    count = split(sys.argv[1], sys.argv[2], int(sys.argv[3]))
    print(f"split-layers: {count} layers")


if __name__ == "__main__":
    main()
