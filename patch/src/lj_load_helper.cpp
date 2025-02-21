#include <cassert>
#include <cctype>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <unistd.h>
#include <unordered_map>

#define PURPLE_COLOR "\033[35m"
#define RESET_COLOR "\033[0m"

#define LJP_INFO(fmt, ...)                                                                                                                                                                                                                                                                                                                                                                                     \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        fprintf(stdout, PURPLE_COLOR " [INFO] " RESET_COLOR fmt, ##__VA_ARGS__);                                                                                                                                                                                                                                                                                                                               \
        fflush(stdout);                                                                                                                                                                                                                                                                                                                                                                                        \
    } while (0)

#define LJP_WARNING(fmt, ...)                                                                                                                                                                                                                                                                                                                                                                                  \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        fprintf(stdout, "[%s:%s:%d]" PURPLE_COLOR " [WARNING] " RESET_COLOR fmt, __FILE__, __func__, __LINE__, ##__VA_ARGS__);                                                                                                                                                                                                                                                                                 \
        fflush(stdout);                                                                                                                                                                                                                                                                                                                                                                                        \
    } while (0)

#define LJP_DEBUG(fmt, ...)                                                                                                                                                                                                                                                                                                                                                                                    \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        fprintf(stdout, "[%s:%s:%d]" PURPLE_COLOR " [DEBUG] " RESET_COLOR fmt, __FILE__, __func__, __LINE__, ##__VA_ARGS__);                                                                                                                                                                                                                                                                                   \
        fflush(stdout);                                                                                                                                                                                                                                                                                                                                                                                        \
    } while (0)

#define LJP_ASSERT(condition, fmt, ...)                                                                                                                                                                                                                                                                                                                                                                        \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        if (!(condition)) {                                                                                                                                                                                                                                                                                                                                                                                    \
            fprintf(stderr, "[%s:%s:%d] Assertion failed: " fmt "\n", __FILE__, __func__, __LINE__, ##__VA_ARGS__);                                                                                                                                                                                                                                                                                            \
            fflush(stderr);                                                                                                                                                                                                                                                                                                                                                                                    \
            exit(EXIT_FAILURE);                                                                                                                                                                                                                                                                                                                                                                                \
        }                                                                                                                                                                                                                                                                                                                                                                                                      \
    } while (0)

extern "C" const char *ljp_file_transform(const char *filename);

typedef struct {
    std::string content;
    uint32_t ptr;
} StringFile;

std::unordered_map<std::string, StringFile> stringMap;

extern "C" void ljp_string_file_reset_ptr(const char *filename) {
    auto it = stringMap.find(filename);
    if (it != stringMap.end()) {
        it->second.ptr = 0;
    } else {
        LJP_ASSERT(false, "File not found: %s\n", filename);
    }
}

extern "C" size_t ljp_string_file_get_content(char *buf, size_t expectSize, const char *filename) {
    auto it = stringMap.find(filename);
    if (it != stringMap.end()) {
        auto currSize = it->second.content.size() - it->second.ptr;
        if (currSize < expectSize) {
            std::copy(it->second.content.begin() + it->second.ptr, it->second.content.begin() + it->second.ptr + currSize, buf);
            it->second.ptr = it->second.content.size();
            return currSize;
        } else {
            std::copy(it->second.content.begin() + it->second.ptr, it->second.content.begin() + it->second.ptr + expectSize, buf);
            it->second.ptr += expectSize;
            return expectSize;
        }
    } else {
        LJP_ASSERT(false, "File not found: %s\n", filename);
    }
}

extern "C" char ljp_string_file_check_eof(const char *filename) {
    auto it = stringMap.find(filename);
    if (it != stringMap.end()) {
        if (it->second.ptr == it->second.content.size()) {
            return 1;
        } else {
            return 0;
        }
    } else {
        LJP_ASSERT(false, "File not found: %s\n", filename);
    }
}

// Interface functions for lj_load.c
extern "C" {
const char *transform_lua(const char *file_path);

const char *ljp_file_transform(const char *filename) {

    auto content     = transform_lua(filename);
    auto filenameStr = std::string(filename);
    if (stringMap.find(filenameStr) == stringMap.end()) {
        stringMap.emplace(filenameStr, StringFile{std::string(content), 0});
    } else {
        LJP_ASSERT(false, "Duplicate file: %s", filename);
    }

    return filename;
}

void ljp_string_transform(const char *str, size_t *output_size) {
    // TODO:
    // std::string inputString(str);
    // std::cout << "[Debug]transformedString => \n>>>\n" << inputString << "\n<<<" << std::endl;
    // *output_size = inputString.size();
    // str = (const char *)inputString.c_str();
}
}
