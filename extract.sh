#!/bin/bash
# utencore 源码提取脚本 - 平铺 + 改后缀方便上传

SRC_DIR="/mnt/g/utencore"
OUT_DIR="/tmp/utencore_flat"

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*

# 关键文件列表
files=(
    # 核心 VM
    "utencore-core/src/vm/dispatch.rs"
    "utencore-core/src/vm/dispatch.rs.bak"
    "utencore-core/src/vm/mod.rs"
    "utencore-core/src/vm/gc.rs"
    "utencore-core/src/vm/call.rs"
    "utencore-core/src/vm/stdlib.rs"
    "utencore-core/src/vm/ns.rs"
    "utencore-core/src/vm/unsafe_.rs"
    
    # 指令集
    "utencore-core/src/opcodes/mod.rs"
    "utencore-core/src/opcodes/info.rs"
    "utencore-types/src/opcodes/mod.rs"
    "utencore-types/src/opcodes/info.rs"
    
    # 编译器
    "compilers/py2uc/src/codegen.rs"
    "compilers/py2uc/src/lib.rs"
    "compilers/py2uc/src/ast.rs"
    "compilers/py2uc/src/parser.rs"
    "compilers/py2uc/src/tokenizer.rs"
    "compilers/py2uc/src/tokenizer_handlers.rs"
    
    # GC
    "utencore-gc/src/lib.rs"
    "utencore-gc/src/memory/mod.rs"
    "utencore-gc/src/memory/mark_sweep.rs"
    "utencore-gc/src/memory/refcount.rs"
    
    # FFI/CIB
    "utencore-core/src/cib/mod.rs"
    "utencore-core/src/cib/ffi.rs"
    "utencore-core/src/cib/marshal.rs"
    "utencore-core/src/cib/structs.rs"
    "utencore-core/src/cib/ucif.rs"
    
    # 其他核心
    "utencore-core/src/lib.rs"
    "utencore-core/src/ir.rs"
    "utencore-core/src/jit.rs"
    "utencore-core/src/plugin.rs"
    "utencore-core/src/ccis.rs"
    "utencore-core/src/ucsl.rs"
    "utencore-bytecode/src/bytecode.rs"
    "utencore-bytecode/src/lib.rs"
    "utencore-types/src/lib.rs"
    "utencore-types/src/types.rs"
    "utencore-types/src/error.rs"
    
    # 测试
    "utencore-core/tests/vm_tests.rs"
    "utencore-core/tests/bytecode_tests.rs"
    "compilers/py2uc/tests/compile_tests.rs"
    
    # Cargo.toml
    "Cargo.toml"
    "utencore-core/Cargo.toml"
    "compilers/py2uc/Cargo.toml"
    "utencore-gc/Cargo.toml"
    "utencore-bytecode/Cargo.toml"
    "utencore-types/Cargo.toml"
    "uc-binaries/Cargo.toml"
    
    # 其他
    "README.md"
    "DESIGN.md"
    "ROADMAP.md"
)

echo "=== 开始提取 ==="
for f in "${files[@]}"; do
    src="$SRC_DIR/$f"
    if [ -f "$src" ]; then
        # 把路径中的 / 替换成 _，加上 .txt 后缀
        flatname=$(echo "$f" | tr '/' '_').txt
        cp "$src" "$OUT_DIR/$flatname"
        lines=$(wc -l < "$src")
        echo "✓ $f ($lines 行) -> $flatname"
    else
        echo "✗ 缺失: $f"
    fi
done

echo ""
echo "=== 统计 ==="
echo "总文件数: $(ls -1 "$OUT_DIR" | wc -l)"
echo "总代码行数: $(cat "$OUT_DIR"/*.txt 2>/dev/null | wc -l)"
echo ""
echo "输出目录: $OUT_DIR"
echo "你可以直接压缩这个目录上传:"
echo "  zip -r /tmp/utencore_flat.zip $OUT_DIR"
