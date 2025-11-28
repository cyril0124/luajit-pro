# LuaJIT-Pro

[![CI](https://github.com/cyril0124/luajit-pro/actions/workflows/ci.yml/badge.svg)](https://github.com/cyril0124/luajit-pro/actions/workflows/ci.yml)

> **Note**: Some parts of this codebase were generated with the assistance of AI tools including GitHub Copilot/Claude.

`LuaJIT-Pro` is a LuaJIT fork with extended syntax and compile-time code generation capabilities. It is based on [the OpenResty fork of LuaJIT 2.1.0](https://github.com/openresty/luajit2).

## Features

- **Compile-time Code Generation**: Generate Lua code at compile time using `__LJP:COMP_TIME()` functions
- **File Include System**: Include other Lua files directly with `__ljp:include()` and `__LJP:INCLUDE()`
- **Compile-time Enums**: Define compile-time constant enums with `--[[@comp_time_enum]]`
- **Teal Language Support**: Write type-safe Lua code using Teal syntax
- **Luau Support**: Basic Luau syntax support
- **Code Formatting**: Optional code formatting with `format` or `pretty` flags
- **Comment Removal**: Remove comments from output with `no-comment` flag
- **Feature Flags**: Conditional compilation using parameter tables

## Installation

### Prerequisites

- Rust (stable toolchain)
- GCC/Clang with C++ support (for building LuaJIT)
- GNU Make

### Build

```bash
# Clone the repository
git clone https://github.com/cyril0124/luajit-pro.git
cd luajit-pro

# 1. Build Rust library
cargo build --release

# 2. Build LuaJIT with patches
bash init.sh
```

The built LuaJIT binary will be located at `luajit2.1/bin/luajit`.

## Usage

### Basic Usage

Add `--[[luajit-pro]]` at the **beginning of your Lua file** to enable luajit-pro features:

```lua
--[[luajit-pro]]

function __LJP:COMP_TIME() -- or `function __ljp:comp_time()`
    return [[
        print("This code is generated at compile time!")
    ]]
end
```

### Configuration Flags

You can add configuration flags in the first line comment:

```lua
--[[luajit-pro, no-cache, opt, format, no-comment, {FEATURE = 1}]]
```

Available flags:
- `no-cache`: Disable caching (always recompile)
- `opt`: Enable code optimization
- `format`: Format the output code
- `no-comment`: Remove comments from output
- `pretty`: Combine `no-comment` and `format`
- `teal`: Enable Teal language syntax
- `luau`: Enable Luau language syntax
- `{KEY = VALUE, ...}`: Parameter table for feature flags

### Compile-time Functions

#### `__LJP:COMP_TIME()`

Execute code at compile time and inject the returned string as Lua code:

```lua
--[[luajit-pro]]

function __LJP:COMP_TIME()
    local code = ""
    for i = 1, 10 do
        code = code .. string.format("local var_%d = %d\n", i, i * 10)
    end
    return code
end

-- The above generates:
-- local var_1 = 10
-- local var_2 = 20
-- ... etc
```

#### `_G.__LJP:comp_time(code_block_name)`

Same as `COMP_TIME` but accepts an optional code block name parameter for debugging. When `print()` is called inside the compile-time function, the code block name will be displayed in the output:

```lua
--[[luajit-pro]]

function _G.__LJP:comp_time(my_config)
    print("Generating code...")  -- Output: [comp_time] /path/to/file.lua my_config    Generating code...
    return [[
        print("Generated code executed")
    ]]
end
```

The code block name is set to `_G.__code_name__` during execution and is useful for identifying which compile-time block is being executed when debugging.

### Include System

Include other Lua files directly:

```lua
--[[luajit-pro]]

__ljp:include("path/to/module")
__LJP:INCLUDE("path/to/another_module")
_G.__ljp:include("path/to/third_module")
```

### Compile-time Enums

Define compile-time constant enums:

```lua
--[[luajit-pro]]

local --[[@comp_time_enum]] Colors = {
    RED = 1,
    GREEN = 2,
    BLUE = 3
}

-- At compile time, Colors.RED is replaced with 1
if color == Colors.RED then
    print("It's red!")
end
```

### Feature Flags

Use parameter tables for conditional compilation:

```lua
--[[luajit-pro, {DEBUG = 0, RELEASE = 1}]]

if _G.DEBUG then
    print("Debug mode enabled")
end

if _G.RELEASE then
    print("Release mode enabled")
end
```

Override at runtime with environment variables:
```bash
DEBUG=1 luajit your_script.lua
```

### Teal Language Support

Write type-safe Lua code:

```lua
--[[luajit-pro, teal]]

local a: number = 42
local b: string = "hello"

local record Point
    x: number
    y: number
end

local p: Point = { x = 10, y = 20 }
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `LJP_NO_CACHE=1` | Disable all caching |
| `LJP_GEN_ONLY=1` | Generate transformed code without executing |
| `LJP_OUT_DIR=<path>` | Custom output directory for cached files |
| `LJP_NO_OPT=1` | Disable optimization even when `opt` flag is set |

## Project Structure

```
luajit-pro/
├── src/                    # Rust source code
│   ├── lib.rs              # Main library entry point
│   ├── lua_transformer.rs  # AST transformation logic
│   ├── lua_optimizer.rs    # Code optimization
│   ├── lang_utils.rs       # Language utilities (Teal, Luau)
│   └── ast_utilis.rs       # AST utilities
├── patch/                  # LuaJIT patches
│   └── src/
│       ├── lj_load.c       # Patched file loader
│       ├── lj_load_helper.cpp # C++ helper for Rust FFI
│       ├── Makefile        # Patched Makefile
│       └── Makefile.dep    # Patched dependencies
├── luajit2.1/              # LuaJIT 2.1 source (OpenResty fork)
├── tests/                  # Test suite
│   ├── run_tests.sh        # Test runner script
│   └── test_*.lua          # Test files
├── tl/                     # Teal language source
├── .github/
│   └── workflows/
│       └── ci.yml          # GitHub Actions CI configuration
├── Cargo.toml              # Rust dependencies
├── init.sh                 # Build script
└── install_luajit.sh       # LuaJIT installation script
```

## How It Works

LuaJIT-Pro adds a syntax transformer layer in `lj_load.c` which is the entry point of LuaJIT's file loader and string loader. When a file is loaded:

1. The first line is checked for the `--[[luajit-pro]]` marker
2. If found, the file is passed to the Rust-based syntax transformer
3. The transformer processes compile-time functions, includes, and other extensions
4. The transformed Lua code is then parsed and compiled by LuaJIT

See [lj_load_helper.cpp](patch/src/lj_load_helper.cpp) and [lib.rs](src/lib.rs) for detailed implementation.

## Testing

Run the test suite:

```bash
# Run all tests (Lua + Golden file tests)
bash tests/run_tests.sh

# Run only Lua tests
bash tests/run_tests.sh lua

# Run only Golden file tests
bash tests/run_tests.sh golden

# Run only Teal tests (requires tl module)
bash tests/run_tests.sh teal

# Update golden files (after intentional changes)
bash tests/run_tests.sh update-golden
```
