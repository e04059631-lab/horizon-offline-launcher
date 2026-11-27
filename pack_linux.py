#!/usr/bin/env python3
"""Packs a compiled Linux horizon-offline-launcher ELF binary with payload_linux.tar.gz."""
import os
import struct
import sys

MAGIC = b"HORIZON_PAYLOAD\x00"


def main() -> None:
    if len(sys.argv) < 4:
        print(f"Usage: {sys.argv[0]} <linux_elf_bin> <payload_linux.tar.gz> <output_bin>")
        sys.exit(1)

    elf_path, payload_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    payload_size = os.path.getsize(payload_path)
    tmp_path = out_path + ".packing"

    with open(tmp_path, "wb") as out:
        with open(elf_path, "rb") as f:
            while chunk := f.read(4 * 1024 * 1024):
                out.write(chunk)
        with open(payload_path, "rb") as f:
            while chunk := f.read(4 * 1024 * 1024):
                out.write(chunk)
        out.write(struct.pack("<Q", payload_size))
        out.write(MAGIC)

    os.replace(tmp_path, out_path)
    try:
        os.chmod(out_path, 0o755)
    except OSError:
        pass

    final_size = os.path.getsize(out_path)
    # Self-verify footer
    with open(out_path, "rb") as f:
        f.seek(-24, 2)
        footer = f.read(24)
        stored_len = struct.unpack("<Q", footer[:8])[0]
        assert footer[8:] == MAGIC, "Magic mismatch"
        assert stored_len == payload_size, "Payload length mismatch"

    print(
        f"[OK] Packed standalone Linux binary: {out_path} "
        f"({final_size / 1048576:.2f} MB, payload {payload_size / 1048576:.2f} MB)"
    )


if __name__ == "__main__":
    main()
