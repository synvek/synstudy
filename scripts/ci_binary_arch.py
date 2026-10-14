#!/usr/bin/env python3
"""打印可执行文件的 CPU 架构（Mach-O / ELF / PE），供 CI 校验安装包与目标三元组一致。

用法：
    python3 scripts/ci_binary_arch.py <文件> [<文件> ...]

输出：每行 "<arch>\t<文件>"，arch ∈ {x86_64, arm64, universal, unknown}。
退出码：任一文件识别为 unknown（或读不了）时返回 1，其余为 0。

为什么需要：dx 的 bundle 产物目录只按 OS 归档、不按 triple 分（target/dx/synstudy/bundle/
<os>），CI 同机跑多架构时容易把上一次的产物当成本次的。用文件头判定架构比看文件名可靠：
macOS 交叉编译时 host 是 arm64，但产物必须是 x86_64。

不依赖任何第三方模块，也不依赖 `file`/`objdump`（Windows runner 上未必有）。
"""

from __future__ import annotations

import struct
import sys

X86_64 = "x86_64"
ARM64 = "arm64"

# Mach-O cputype（高位 ABI64 位需先清掉）
_MACHO_CPU = {7: X86_64, 12: ARM64}  # CPU_TYPE_X86_64 / CPU_TYPE_ARM（arm64 是 ARM|ABI64）
_ELF_MACHINE = {62: X86_64, 183: ARM64}  # EM_X86_64 / EM_AARCH64
_PE_MACHINE = {0x8664: X86_64, 0xAA64: ARM64}  # IMAGE_FILE_MACHINE_AMD64 / _ARM64


def _macho_arch(cputype: int) -> str:
    """cputype 低 24 位是架构编号，高位的 ABI64 标志要忽略（arm64e 亦归为 arm64）。"""
    return _MACHO_CPU.get(cputype & 0x00FFFFFF, f"cpu{cputype & 0x00FFFFFF}")


def arch_of(path: str) -> str:
    with open(path, "rb") as fh:
        head = fh.read(4096)
    if len(head) < 20:
        return "unknown"

    magic = head[:4]

    # ── Mach-O thin：0xfeedfacf(64 位小端) / 0xfeedface(32 位小端) 及对应大端 ──
    if magic in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe"):
        return _macho_arch(struct.unpack("<I", head[4:8])[0])
    if magic in (b"\xfe\xed\xfa\xcf", b"\xfe\xed\xfa\xce"):
        return _macho_arch(struct.unpack(">I", head[4:8])[0])

    # ── Mach-O fat（universal）：magic 大端，随后 nfat_arch + fat_arch[]（每项 20B，全大端）
    if magic in (b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"):
        n = struct.unpack(">I", head[4:8])[0]
        archs = []
        for i in range(min(n, 8)):
            off = 8 + i * 20
            if off + 4 > len(head):
                break
            archs.append(_macho_arch(struct.unpack(">I", head[off : off + 4])[0]))
        uniq = sorted(set(archs))
        if len(uniq) > 1:
            return "universal"
        return uniq[0] if uniq else "unknown"

    # ── ELF：e_machine 在偏移 18（2 字节，端序由 EI_DATA=head[5] 决定）──
    if magic == b"\x7fELF":
        fmt = ">H" if head[5] == 2 else "<H"
        machine = struct.unpack(fmt, head[18:20])[0]
        return _ELF_MACHINE.get(machine, f"elf{machine}")

    # ── PE：DOS 头 0x3C 处是 e_lfanew，其后 'PE\0\0' + 2 字节 Machine ──
    if magic == b"MZ":
        pe_off = struct.unpack("<I", head[0x3C:0x40])[0]
        if pe_off + 6 <= len(head) and head[pe_off : pe_off + 4] == b"PE\x00\x00":
            machine = struct.unpack("<H", head[pe_off + 4 : pe_off + 6])[0]
            return _PE_MACHINE.get(machine, f"pe{machine:#x}")

    return "unknown"


def main(argv: list[str]) -> int:
    if not argv:
        print("用法: ci_binary_arch.py <文件> [<文件> ...]", file=sys.stderr)
        return 2

    rc = 0
    for path in argv:
        try:
            arch = arch_of(path)
        except OSError as exc:
            print(f"# 读取失败 {path}: {exc}", file=sys.stderr)
            rc = 1
            continue
        if arch == "unknown":
            rc = 1
        print(f"{arch}\t{path}")
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
