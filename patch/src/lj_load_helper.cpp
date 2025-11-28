/**
 * @file lj_load_helper.cpp
 * @brief LuaJIT-Pro file transformation helper
 *
 * This file provides a bridge between LuaJIT's file loading system and
 * the Rust-based luajit_pro_helper library. It manages transformed Lua
 * source code in memory, allowing LuaJIT to read the processed content
 * as if from a file.
 *
 * Architecture:
 * 1. LuaJIT calls ljp_file_transform() with a filename
 * 2. The Rust transform_lua() function processes the file
 * 3. Transformed content is stored in stringMap
 * 4. LuaJIT reads content via ljp_string_file_get_content()
 */

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <unordered_map>

// ============================================================================
// Logging Macros
// ============================================================================

#define PURPLE_COLOR "\033[35m"
#define RESET_COLOR  "\033[0m"

#define LJP_INFO(fmt, ...) \
    do { \
        fprintf(stdout, PURPLE_COLOR " [INFO] " RESET_COLOR fmt, ##__VA_ARGS__); \
        fflush(stdout); \
    } while (0)

#define LJP_WARNING(fmt, ...) \
    do { \
        fprintf(stdout, "[%s:%s:%d]" PURPLE_COLOR " [WARNING] " RESET_COLOR fmt, \
                __FILE__, __func__, __LINE__, ##__VA_ARGS__); \
        fflush(stdout); \
    } while (0)

#define LJP_DEBUG(fmt, ...) \
    do { \
        fprintf(stdout, "[%s:%s:%d]" PURPLE_COLOR " [DEBUG] " RESET_COLOR fmt, \
                __FILE__, __func__, __LINE__, ##__VA_ARGS__); \
        fflush(stdout); \
    } while (0)

#define LJP_ASSERT(condition, fmt, ...) \
    do { \
        if (!(condition)) { \
            fprintf(stderr, "[%s:%s:%d] Assertion failed: " fmt "\n", \
                    __FILE__, __func__, __LINE__, ##__VA_ARGS__); \
            fflush(stderr); \
            exit(EXIT_FAILURE); \
        } \
    } while (0)

// ============================================================================
// Types
// ============================================================================

/**
 * @brief Represents an in-memory file with read position tracking
 *
 * This structure mimics a file stream, allowing sequential reads from
 * a string buffer. The ptr field tracks the current read position.
 */
struct StringFile {
    std::string content;  ///< The file content
    uint32_t ptr;         ///< Current read position
};

// ============================================================================
// Global State
// ============================================================================

/// Maps filenames to their transformed content
static std::unordered_map<std::string, StringFile> stringMap;

// ============================================================================
// External Declarations
// ============================================================================

extern "C" {
/// Rust function to transform Lua source code
const char* transform_lua(const char* file_path);
}

// ============================================================================
// String File Interface
// ============================================================================

/**
 * @brief Reset the read pointer for a string file
 * @param filename The filename to reset
 *
 * This allows re-reading a file from the beginning.
 */
extern "C" void ljp_string_file_reset_ptr(const char* filename) {
    auto it = stringMap.find(filename);
    LJP_ASSERT(it != stringMap.end(), "File not found: %s", filename);
    it->second.ptr = 0;
}

/**
 * @brief Read content from a string file
 * @param buf Output buffer
 * @param expect_size Maximum bytes to read
 * @param filename The filename to read from
 * @return Number of bytes actually read
 *
 * Works like fread(): reads up to expect_size bytes into buf,
 * advances the read pointer, and returns the actual bytes read.
 */
extern "C" size_t ljp_string_file_get_content(char* buf, size_t expect_size, const char* filename) {
    auto it = stringMap.find(filename);
    LJP_ASSERT(it != stringMap.end(), "File not found: %s", filename);

    StringFile& file = it->second;
    size_t remaining = file.content.size() - file.ptr;
    size_t read_size = (remaining < expect_size) ? remaining : expect_size;

    std::memcpy(buf, file.content.data() + file.ptr, read_size);
    file.ptr += static_cast<uint32_t>(read_size);

    return read_size;
}

/**
 * @brief Check if end-of-file has been reached
 * @param filename The filename to check
 * @return 1 if EOF, 0 otherwise
 */
extern "C" char ljp_string_file_check_eof(const char* filename) {
    auto it = stringMap.find(filename);
    LJP_ASSERT(it != stringMap.end(), "File not found: %s", filename);

    return (it->second.ptr >= it->second.content.size()) ? 1 : 0;
}

// ============================================================================
// File Transformation Interface
// ============================================================================

/**
 * @brief Transform a Lua file and store the result
 * @param filename Path to the Lua file
 * @return The filename (same as input)
 *
 * This function:
 * 1. Calls the Rust transform_lua() function
 * 2. Stores the transformed content in stringMap
 * 3. Returns the filename for LuaJIT to use with get_content()
 *
 * Note: If the file was already transformed, a warning is logged
 * but the existing content is preserved.
 */
extern "C" const char* ljp_file_transform(const char* filename) {
    const char* content = transform_lua(filename);
    std::string filename_str(filename);

    auto [it, inserted] = stringMap.try_emplace(
        filename_str,
        StringFile{std::string(content), 0}
    );

    if (!inserted) {
        LJP_WARNING("Duplicate file: %s in stringMap\n", filename);
    }

    return filename;
}

/**
 * @brief Transform a Lua string (not yet implemented)
 * @param str Input Lua source string
 * @param output_size Output parameter for transformed size
 *
 * TODO: Implement string transformation for dostring() support
 */
extern "C" void ljp_string_transform(const char* str, size_t* output_size) {
    (void)str;
    (void)output_size;
    // TODO: Implement when needed
}
