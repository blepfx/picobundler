# zig-cross

Example of using [zig](https://ziglang.org) as a CMake Toolchain for cross compiling.

Reference: https://zig.news/kristoff/cross-compile-a-c-c-project-with-zig-3599

## Building

- [Install zig](https://ziglang.org/learn/getting-started/#installing-zig) in your PATH (`choco install zig` on Windows)
- `cmake -B build-aarch64 -G Ninja --toolchain aarch64-linux-gnu.cmake`
- `cmake --build build-arch64`

You can create toolchains for other triples as well, just create a file named `aarch64-windows-gnu.cmake` with the following contents to build for Windows on ARM64:

```cmake
include(${CMAKE_CURRENT_LIST_DIR}/cmake/zig-toolchain.cmake)
```

## clangd

To get [clangd](https://clangd.llvm.org/) to work you need to first enable generation of `compile_commands.json`:

```sh
cmake -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON
```

Additionally you need to pass pass the following arguments to `clangd`:

```json
"clangd.arguments": [
    "--log=verbose",
    "--query-driver=**/zig-cc.cmd,**/zig-cc,**/zig-c++.cmd,**/zig-c++",
]
```

Without these arguments `clangd` will not query the driver (`zig c++`) and the include paths will not be resolved correctly.
