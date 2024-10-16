#include <cassert>
#include <cctype>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <ostream>
#include <regex>
#include <sstream>
#include <string>
#include <unistd.h>
#include <unordered_set>
#include <vector>

#define PURPLE_COLOR "\033[35m"
#define RESET_COLOR "\033[0m"

#define LJ_PRO_CACHE_DIR "./.luajit_pro"
#define LJ_PRO_BUILD_CACHE_DIR "./.luajit_pro/.build_cache"

typedef const char *(*LuaDoStringPtr)(const char *, const char *);

#define LJP_INFO(fmt, ...)                                                                                                                                                                                                                                                                                                                                                                                         \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        fprintf(stdout, PURPLE_COLOR " [INFO] " RESET_COLOR fmt, ##__VA_ARGS__);                                                                                                                                                                                                                                                                                                                                 \
        fflush(stdout);                                                                                                                                                                                                                                                                                                                                                                                        \
    } while (0)

#define LJP_WARNING(fmt, ...)                                                                                                                                                                                                                                                                                                                                                                                        \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        fprintf(stdout, "[%s:%s:%d]" PURPLE_COLOR " [WARNING] " RESET_COLOR fmt, __FILE__, __func__, __LINE__, ##__VA_ARGS__);                                                                                                                                                                                                                                                                                     \
        fflush(stdout);                                                                                                                                                                                                                                                                                                                                                                                        \
    } while (0)

#define LJP_DEBUG(fmt, ...)                                                                                                                                                                                                                                                                                                                                                                                        \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        fprintf(stdout, "[%s:%s:%d]" PURPLE_COLOR " [DEBUG] " RESET_COLOR fmt, __FILE__, __func__, __LINE__, ##__VA_ARGS__);                                                                                                                                                                                                                                                                                     \
        fflush(stdout);                                                                                                                                                                                                                                                                                                                                                                                        \
    } while (0)

#define LJP_ASSERT(condition, fmt, ...)                                                                                                                                                                                                                                                                                                                                                                            \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        if (!(condition)) {                                                                                                                                                                                                                                                                                                                                                                                    \
            fprintf(stderr, "[%s:%s:%d] Assertion failed: " fmt "\n", __FILE__, __func__, __LINE__, ##__VA_ARGS__);                                                                                                                                                                                                                                                                                            \
            fflush(stderr);                                                                                                                                                                                                                                                                                                                                                                                    \
            exit(EXIT_FAILURE);                                                                                                                                                                                                                                                                                                                                                                                \
        }                                                                                                                                                                                                                                                                                                                                                                                                      \
    } while (0)

extern "C" const char *file_transform(const char *filename, LuaDoStringPtr func);

namespace lua_transformer {
std::vector<std::string> removeFiles;

LuaDoStringPtr luaDoString = nullptr; // Used for generate compile time code

enum class TokenKind {
    Identifier,
    Foreach,
    Map,
    Filter,
    ZipWithIndex,
    Return,
    Number,
    Symbol,
    CompTime,
    Include,
    EndOfFile,
    Unknown,
};

enum class ForeachKind {
    Foreach,
    ForeachSimple,
    ForeachZipWithIndex,
    ZipWithIndexForeach,
};

enum class MapKind {
    Map,
    MapSimple,
    MapZipWithIndex,
    ZipWithIndexMap,
};

enum class FilterKind {
    Filter,
    FilterSimple,
    FilterZipWithIndex,
    ZipWithIndexFilter,
};

struct Token {
    TokenKind kind;
    std::string data;

    int idx; // Token index
    int startLine;
    int startColumn;
    int endLine;
    int endColumn;

    Token() : kind(TokenKind::Unknown), data(""), idx(0), startLine(0), startColumn(0), endLine(0), endColumn(0) {}
    Token(TokenKind kind, const std::string &data, int startLine, int startColumn, int endLine, int endColumn) : kind(kind), data(data), idx(0 /* index is assigned in nextToken() */), startLine(startLine), startColumn(startColumn), endLine(endLine), endColumn(endColumn) {}
};

std::string toString(TokenKind kind) {
    switch (kind) {
    case TokenKind::Identifier:
        return "Identifier";
    case TokenKind::Foreach:
        return "Foreach";
    case TokenKind::Map:
        return "Map";
    case TokenKind::Filter:
        return "Filter";
    case TokenKind::ZipWithIndex:
        return "ZipWithIndex";
    case TokenKind::Return:
        return "Return";
    case TokenKind::Number:
        return "Number";
    case TokenKind::Symbol:
        return "Symbol";
    case TokenKind::CompTime:
        return "CompTime";
    case TokenKind::Include:
        return "Include";
    case TokenKind::EndOfFile:
        return "EndOfFile";
    case TokenKind::Unknown:
        return "Unknown";
    default:
        return "Unknown TokenKind";
    }
}

class CustomLuaTransformer {
  public:
    std::vector<std::string> oldContentLines;

    explicit CustomLuaTransformer(const std::string &filename);
    void tokenize();
    void parse(int idx);
    void dumpContentLines(bool hasLineNumbers);

  private:
    bool isFirstToken = true;
    std::istream *stream_;
    std::ifstream fstream_;
    std::string filename_;

    std::vector<Token> tokenVec;
    std::unordered_set<int> processedTokenLines;
    std::unordered_set<int> processedTokenColumns;
    std::unordered_set<int> replacedTokenLines;
    std::unordered_set<int> replacedTokenColumns;
    int tokenVecIdx = 0;

    std::vector<int> bracketStack;

    int currentLine_   = 1;
    int currentColumn_ = 0;

    Token _nextToken();
    Token nextToken();

    std::string getContentBetween(Token &startToken, Token &endToken);
    void replaceContentBetween(int line, Token &startToken, Token &endToken, std::string content);
    void replaceContent(int line, std::string content);

    void parseForeach(int idx);
    void parseMap(int idx);
    void parseFilter(int idx);
    void parseCompTime(int idx);
    void parseInclude(int idx);
};

CustomLuaTransformer::CustomLuaTransformer(const std::string &filename) : filename_(filename) {
    fstream_ = std::ifstream(filename);
    if (!std::filesystem::exists(filename)) {
        LJP_ASSERT(false, "[CustomLuaTransformer] file does not exist: %s", filename.c_str());
    }
    stream_ = &fstream_;

    std::ifstream file(filename);
    std::string line;

    if (!file.is_open()) {
        LJP_ASSERT(false, "[CustomLuaTransformer] Unable to open: %s", filename.c_str());
    }

    while (std::getline(file, line)) {
        oldContentLines.push_back(line);
    }

    if (oldContentLines[0].find(std::string("--[[luajit-pro]]")) == std::string::npos) {
        LJP_ASSERT(false, "File does not contain verilua comment in first line: %s", filename.c_str());
    } else {
        oldContentLines[0] = "--[[luajit-pro]] local ipairs, _tinsert = ipairs, table.insert";
    }

    file.close();
}

Token CustomLuaTransformer::_nextToken() {
    auto &stream = *stream_;

    if (stream.bad())
        return Token(TokenKind::EndOfFile, "", currentLine_, currentColumn_, currentLine_, currentColumn_);

    std::stringstream result;
    char c;
    int startLine   = currentLine_;
    int startColumn = currentColumn_;

    // Skip whitespace and update position
    while (stream.get(c) && std::isspace(c)) {
        if (c == '\n') {
            currentLine_++;
            currentColumn_ = 0;
        } else {
            currentColumn_++;
        }
    }

    if (stream.eof())
        return Token(TokenKind::EndOfFile, "", startLine, startColumn, currentLine_, currentColumn_);

    startLine   = currentLine_;
    startColumn = currentColumn_;

    // Handle comments
    if (c == '-') {
        if (stream.peek() == '-') {
            stream.get(c); // consume second '-'
            // Check for long comment
            if (stream.peek() == '[') {
                stream.get(c); // consume '['
                if (stream.peek() == '[') {
                    // Long comment
                    stream.get(c); // consume second '['
                    while (stream.get(c)) {
                        if (c == ']' && stream.peek() == ']') {
                            stream.get(c); // consume closing ']'
                            break;
                        }
                        if (c == '\n') {
                            currentLine_++;
                            currentColumn_ = 0;
                        }
                    }
                    return _nextToken();
                } else {
                    // Single-line comment
                    while (stream.get(c) && c != '\n')
                        ;
                    currentLine_++;
                    currentColumn_ = 0;
                    return _nextToken();
                }
            } else {
                // Single-line comment
                while (stream.get(c) && c != '\n')
                    ;
                currentLine_++;
                currentColumn_ = 0;
                return _nextToken();
            }
        }
    }

    // Handle numbers
    if (std::isdigit(c)) {
        result << c;
        currentColumn_++;
        while (stream.get(c) && std::isdigit(c)) {
            result << c;
            currentColumn_++;
        }
        stream.unget();
        return Token(TokenKind::Number, result.str(), startLine, startColumn, currentLine_, currentColumn_);
    }

    // Handle identifiers and keywords
    if (std::isalpha(c) || c == '_') {
        result << c;
        currentColumn_++;
        while (stream.get(c) && (std::isalnum(c) || c == '_')) {
            result << c;
            currentColumn_++;
        }
        stream.unget();

        if (result.str() == "foreach") {
            return Token(TokenKind::Foreach, result.str(), startLine, startColumn, currentLine_, currentColumn_);
        } else if (result.str() == "map") {
            return Token(TokenKind::Map, result.str(), startLine, startColumn, currentLine_, currentColumn_);
        } else if (result.str() == "filter") {
            return Token(TokenKind::Filter, result.str(), startLine, startColumn, currentLine_, currentColumn_);
        } else if (result.str() == "return") {
            return Token(TokenKind::Return, result.str(), startLine, startColumn, currentLine_, currentColumn_);
        } else if (result.str() == "zipWithIndex") {
            return Token(TokenKind::ZipWithIndex, result.str(), startLine, startColumn, currentLine_, currentColumn_);
        } else {
            return Token(TokenKind::Identifier, result.str(), startLine, startColumn, currentLine_, currentColumn_);
        }
    }

    // Handle $ identifiers
    if (c == '$') {
        result << c;
        currentColumn_++;
        while (stream.get(c) && (std::isalnum(c) || c == '_')) {
            result << c;
            currentColumn_++;
        }
        stream.unget(); // Put back the last character

        // Check if the token is $comp_time
        if (result.str() == "$comp_time") {
            return Token(TokenKind::CompTime, result.str(), startLine, startColumn, currentLine_, currentColumn_);
        } else if (result.str() == "$include") {
            return Token(TokenKind::Include, result.str(), startLine, startColumn, currentLine_, currentColumn_);
        } else {
            return Token(TokenKind::Symbol, result.str(), startLine, startColumn, currentLine_, currentColumn_);
        }
    }

    // Handle symbols
    result << c;
    currentColumn_++;
    if (c == '=' && stream.peek() == '=') {
        stream.get(c);
        result << c;
        currentColumn_++;
    }

    return Token(TokenKind::Symbol, result.str(), startLine, startColumn, currentLine_, currentColumn_);
}

Token CustomLuaTransformer::nextToken() {
    if (isFirstToken) {
        isFirstToken = false;
    } else {
        tokenVecIdx++;
    }

    auto token = _nextToken();
    token.idx  = tokenVecIdx;
    // fmt::println("[{:3}] | {:>8} | {:>15} | {:5} | {:5} |", tokenVecIdx, token.data, toString(token.kind), token.startLine, token.startColumn);

    tokenVec.emplace_back(token);

    return token;
}

void CustomLuaTransformer::tokenize() {
    while (true) {
        auto token = nextToken();
        if (token.kind == TokenKind::EndOfFile)
            break;
    }
}

std::string CustomLuaTransformer::getContentBetween(Token &startToken, Token &endToken) {
    std::string content;
    if (startToken.startLine == endToken.startLine) {
        content += oldContentLines[startToken.startLine - 1].substr(startToken.endColumn, endToken.startColumn - startToken.endColumn);
    } else {
        for (int i = startToken.startLine; i <= endToken.startLine; i++) {
            if (i == startToken.startLine) {
                content += oldContentLines[i - 1].substr(startToken.endColumn);
                content += "\n";
            } else if (i == endToken.startLine) {
                content += oldContentLines[i - 1].substr(0, endToken.startColumn);
                content += "\n";
            } else {
                content += oldContentLines[i - 1];
                content += "\n";
            }
        }
    }
    return content;
}

void CustomLuaTransformer::parseForeach(int idx) {
    int bracketCnt = 0;
    int _idx       = idx;

    while (tokenVec.at(_idx).kind != TokenKind::Foreach) {
        _idx++;
        if (tokenVec.at(_idx).kind == TokenKind::EndOfFile) {
            return;
        }
    }

    ForeachKind foreachKind;
    Token tblToken;
    Token refToken;
    Token idxToken;
    // Token leftBracketToken; // TODO: not use
    Token rightBracketToken;
    Token funcToken;
    Token bodyStartToken;
    refToken.data = "ref";
    idxToken.data = "_";

    if (tokenVec.at(_idx - 2).kind == TokenKind::Identifier) {
        if (tokenVec.at(_idx + 2).kind == TokenKind::ZipWithIndex)
            foreachKind = ForeachKind::ForeachZipWithIndex;
        else if (tokenVec.at(_idx + 2).kind == TokenKind::Identifier && tokenVec.at(_idx + 3).kind == TokenKind::Symbol && tokenVec.at(_idx + 3).data == "}")
            foreachKind = ForeachKind::ForeachSimple;
        else
            foreachKind = ForeachKind::Foreach;
    } else if (tokenVec.at(_idx - 2).kind == TokenKind::ZipWithIndex) {
        foreachKind = ForeachKind::ZipWithIndexForeach;
    } else {
        LJP_ASSERT(false, "Unexpected token");
    }

    switch (foreachKind) {
    case ForeachKind::Foreach:
        // <tblToken>.foreach <leftBracketToken> <refToken> => <bodyStartToken> ... <rightBracketToken>
        tblToken       = tokenVec.at(_idx - 2);
        refToken       = tokenVec.at(_idx + 2);
        bodyStartToken = tokenVec.at(_idx + 5);
        _idx++;
        break;
    case ForeachKind::ForeachSimple:
        // <tblToken>.foreach <leftBracketToken> <funcToken> <rightBracketToken>
        tblToken       = tokenVec.at(_idx - 2);
        funcToken      = tokenVec.at(_idx + 2);
        bodyStartToken = funcToken;
        _idx++;
        break;
    case ForeachKind::ForeachZipWithIndex:
        // <tblToken>.foreach.zipWithIndex <leftBracketToken> (<refToken>, <idxToken>) => <bodyStartToken> ... <rightBracketToken>
        tblToken       = tokenVec.at(_idx - 2);
        refToken       = tokenVec.at(_idx + 5);
        idxToken       = tokenVec.at(_idx + 7);
        bodyStartToken = tokenVec.at(_idx + 11);
        _idx += 3;
        break;
    case ForeachKind::ZipWithIndexForeach:
        // <tblToken>.zipWithIndex.foreach <leftBracketToken> (<idxToken>, <refToken>) => <bodyStartToken> ... <rightBracketToken>
        tblToken       = tokenVec.at(_idx - 4);
        refToken       = tokenVec.at(_idx + 5);
        idxToken       = tokenVec.at(_idx + 3);
        bodyStartToken = tokenVec.at(_idx + 9);
        _idx++;
        break;
    default:
        LJP_ASSERT(false, "Unexpected foreach kind");
    }

    if (processedTokenLines.count(tblToken.startLine) > 0 && processedTokenColumns.count(tblToken.startColumn) > 0) {
        return;
    }

    tblToken.data = tblToken.data;

    auto token      = tokenVec.at(_idx);
    auto startToken = token;
    LJP_ASSERT(token.data == "{", "Unexpected token: %s", token.data.c_str());

    while (token.data == "{" || bracketCnt != 0) {
        if (token.data == "}") {
            bracketCnt--;
            if (bracketCnt == 0) {
                break;
            }
        } else if (token.data == "{") {
            bracketCnt++;
            parse(_idx + 1);
        }

        _idx++;
        token = tokenVec.at(_idx);
    }
    rightBracketToken = token; // The final matched token is right bracket

    processedTokenLines.insert(tblToken.startLine);
    processedTokenColumns.insert(tblToken.startColumn);

    // If the tblToken has been replaced, don't replace it again
    if (replacedTokenLines.count(tblToken.startLine) > 0 && replacedTokenColumns.count(tblToken.startColumn) > 0) {
        return;
    } else {
        replacedTokenLines.insert(tblToken.startLine);
        replacedTokenColumns.insert(tblToken.startColumn);
    }

    if (tblToken.startLine == bodyStartToken.startLine) {
        oldContentLines[rightBracketToken.startLine - 1].replace(rightBracketToken.startColumn, rightBracketToken.startColumn - rightBracketToken.endColumn, "end");
        if (foreachKind == ForeachKind::ForeachSimple) {
            oldContentLines[funcToken.startLine - 1].replace(funcToken.startColumn, funcToken.endColumn - funcToken.startColumn, funcToken.data + "(" + refToken.data + ") ");
            oldContentLines[tblToken.startLine - 1].replace(tblToken.startColumn, bodyStartToken.startColumn - tblToken.startColumn, "for " + idxToken.data + ", " + refToken.data + " in ipairs(" + tblToken.data + ") do ");
        } else {
            oldContentLines[tblToken.startLine - 1].replace(tblToken.startColumn, bodyStartToken.startColumn - tblToken.startColumn, "for " + idxToken.data + ", " + refToken.data + " in ipairs(" + tblToken.data + ") do ");
        }
    } else {
        oldContentLines[rightBracketToken.startLine - 1].replace(rightBracketToken.startColumn, rightBracketToken.startColumn - rightBracketToken.endColumn, "end");
        if (foreachKind == ForeachKind::ForeachSimple) {
            oldContentLines[funcToken.startLine - 1].replace(funcToken.startColumn, funcToken.endColumn - funcToken.startColumn, funcToken.data + "(" + refToken.data + ") ");
        }
        oldContentLines[tblToken.startLine - 1] = "for " + idxToken.data + ", " + refToken.data + " in ipairs(" + tblToken.data + ") do ";

        for (int i = tblToken.startLine + 1; i <= bodyStartToken.startLine; i++) {
            if (i == bodyStartToken.startLine) {
                oldContentLines[i - 1].replace(0, bodyStartToken.startColumn, std::string(bodyStartToken.startColumn, ' '));
            } else {
                oldContentLines[i - 1] = "--[[line keeper]]";
            }
        }
    }
}

void CustomLuaTransformer::parseMap(int idx) {
    int bracketCnt = 0;
    int _idx       = idx;

    while (tokenVec.at(_idx).kind != TokenKind::Map) {
        _idx++;
        if (tokenVec.at(_idx).kind == TokenKind::EndOfFile) {
            return;
        }
    }

    MapKind mapKind;
    Token retToken;
    Token returnToken;
    Token tblToken;
    Token refToken;
    Token idxToken;
    // Token leftBracketToken; // TODO: not use
    Token rightBracketToken;
    Token funcToken;
    Token bodyStartToken;
    refToken.data = "ref";
    idxToken.data = "_";

    if (tokenVec.at(_idx - 2).kind == TokenKind::Identifier) {
        if (tokenVec.at(_idx + 2).kind == TokenKind::ZipWithIndex)
            mapKind = MapKind::MapZipWithIndex;
        else if (tokenVec.at(_idx + 2).kind == TokenKind::Identifier && tokenVec.at(_idx + 3).data == "}")
            mapKind = MapKind::MapSimple;
        else
            mapKind = MapKind::Map;
    } else if (tokenVec.at(_idx - 2).kind == TokenKind::ZipWithIndex) {
        mapKind = MapKind::ZipWithIndexMap;
    } else {
        LJP_ASSERT(false, "Unexpected token");
    }

    switch (mapKind) {
    case MapKind::Map:
        // <retToken> = <tblToken>.map <leftBracketToken> <refToken> => <bodyStartToken> ... <returnToken> ... <rightBracketToken>
        retToken       = tokenVec.at(_idx - 4);
        tblToken       = tokenVec.at(_idx - 2);
        refToken       = tokenVec.at(_idx + 2);
        bodyStartToken = tokenVec.at(_idx + 5);
        _idx++;
        break;
    case MapKind::MapSimple:
        // <retToken> = <tblToken>.map <leftBracketToken> <funcToken> <rightBracketToken>
        retToken       = tokenVec.at(_idx - 4);
        tblToken       = tokenVec.at(_idx - 2);
        funcToken      = tokenVec.at(_idx + 2);
        bodyStartToken = funcToken;
        _idx++;
        break;
    case MapKind::MapZipWithIndex:
        // <retToken> = <tblToken>.map.zipWithIndex <leftBracketToken> (<refToken>, <idxToken>) => <bodyStartToken> ... <returnToken> ... <rightBracketToken>
        retToken       = tokenVec.at(_idx - 4);
        tblToken       = tokenVec.at(_idx - 2);
        refToken       = tokenVec.at(_idx + 5);
        idxToken       = tokenVec.at(_idx + 7);
        bodyStartToken = tokenVec.at(_idx + 11);
        _idx += 3;
        break;
    case MapKind::ZipWithIndexMap:
        // <retToken> = <tblToken>.zipWithIndex.map <leftBracketToken> (<idxToken>, <refToken>) => <bodyStartToken> ... <returnToken> ... <rightBracketToken>
        retToken       = tokenVec.at(_idx - 6);
        tblToken       = tokenVec.at(_idx - 4);
        refToken       = tokenVec.at(_idx + 5);
        idxToken       = tokenVec.at(_idx + 3);
        bodyStartToken = tokenVec.at(_idx + 9);
        _idx++;
        break;
    default:
        LJP_ASSERT(false, "Unexpected map kind");
    }

    if (processedTokenLines.count(tblToken.startLine) > 0 && processedTokenColumns.count(tblToken.startColumn) > 0) {
        return;
    }

    auto token      = tokenVec.at(_idx);
    auto startToken = token;
    LJP_ASSERT(token.data == "{", "Unexpected token: %s", token.data.c_str());

    while (token.data == "{" || bracketCnt != 0) {
        if (token.data == "}") {
            bracketCnt--;
            if (bracketCnt == 0) {
                break;
            }
        } else if (token.data == "{") {
            bracketCnt++;
            parseMap(_idx + 1);
        }

        _idx++;
        token = tokenVec.at(_idx);
    }

    // MapSimple does not have return token
    if (mapKind != MapKind::MapSimple) {
        auto tmpIdx = _idx;
        while (tokenVec.at(tmpIdx).kind != TokenKind::Return) {
            tmpIdx--;
            if (tokenVec.at(tmpIdx).idx == tblToken.idx) {
                LJP_ASSERT(false, "Cannot find return token!\n");
            }
        }
        returnToken = tokenVec.at(tmpIdx);
    }
    rightBracketToken = token; // The final matched token is right bracket

    processedTokenLines.insert(tblToken.startLine);
    processedTokenColumns.insert(tblToken.startColumn);

    // If the tblToken has been replaced, don't replace it again
    if (replacedTokenLines.count(tblToken.startLine) > 0 && replacedTokenColumns.count(tblToken.startColumn) > 0) {
        return;
    } else {
        replacedTokenLines.insert(tblToken.startLine);
        replacedTokenColumns.insert(tblToken.startColumn);
    }

    if (tblToken.startLine == bodyStartToken.startLine) {
        oldContentLines[rightBracketToken.startLine - 1].replace(rightBracketToken.startColumn, rightBracketToken.startColumn - rightBracketToken.endColumn, ") end");
        if (mapKind == MapKind::MapSimple) {
            oldContentLines[funcToken.startLine - 1].replace(funcToken.startColumn, funcToken.endColumn - funcToken.startColumn, "_tinsert(" + retToken.data + ", " + funcToken.data + "(" + refToken.data + ") ");
        } else {
            oldContentLines[returnToken.startLine - 1].replace(returnToken.startColumn, returnToken.endColumn - returnToken.startColumn, "_tinsert(" + retToken.data + ",");
        }
        oldContentLines[tblToken.startLine - 1].replace(retToken.startColumn, bodyStartToken.startColumn - retToken.startColumn, retToken.data + " = {}; for " + idxToken.data + ", " + refToken.data + " in ipairs(" + tblToken.data + ") do ");
    } else {
        oldContentLines[rightBracketToken.startLine - 1].replace(rightBracketToken.startColumn, rightBracketToken.startColumn - rightBracketToken.endColumn, ") end");
        oldContentLines[tblToken.startLine - 1] = oldContentLines[tblToken.startLine - 1].substr(0, retToken.startColumn) + retToken.data + " = {}; for " + idxToken.data + ", " + refToken.data + " in ipairs(" + tblToken.data + ") do ";
        if (mapKind == MapKind::MapSimple) {
            oldContentLines[funcToken.startLine - 1].replace(funcToken.startColumn, funcToken.endColumn - funcToken.startColumn, "_tinsert(" + retToken.data + ", " + funcToken.data + "(" + refToken.data + ") ");
        } else {
            oldContentLines[returnToken.startLine - 1].replace(returnToken.startColumn, returnToken.endColumn - returnToken.startColumn, "_tinsert(" + retToken.data + ",");
        }
        for (int i = tblToken.startLine + 1; i <= bodyStartToken.startLine; i++) {
            if (i == bodyStartToken.startLine) {
                oldContentLines[i - 1].replace(0, bodyStartToken.startColumn, std::string(bodyStartToken.startColumn, ' '));
            } else {
                oldContentLines[i - 1] = "--[[line keeper]]";
            }
        }
    }
}

void CustomLuaTransformer::parseFilter(int idx) {
    int bracketCnt = 0;
    int _idx       = idx;

    while (tokenVec.at(_idx).kind != TokenKind::Filter) {
        _idx++;
        if (tokenVec.at(_idx).kind == TokenKind::EndOfFile) {
            return;
        }
    }

    FilterKind filterKind;
    Token retToken;
    Token returnToken;
    Token tblToken;
    Token refToken;
    Token idxToken;
    Token rightBracketToken;
    Token funcToken;
    Token bodyStartToken;
    refToken.data = "ref";
    idxToken.data = "_";

    if (tokenVec.at(_idx - 2).kind == TokenKind::Identifier) {
        if (tokenVec.at(_idx + 2).kind == TokenKind::ZipWithIndex)
            filterKind = FilterKind::FilterZipWithIndex;
        else if (tokenVec.at(_idx + 2).kind == TokenKind::Identifier && tokenVec.at(_idx + 3).data == "}")
            filterKind = FilterKind::FilterSimple;
        else
            filterKind = FilterKind::Filter;
    } else if (tokenVec.at(_idx - 2).kind == TokenKind::ZipWithIndex) {
        filterKind = FilterKind::ZipWithIndexFilter;
    } else {
        // fmt::println("Unexpected token at line => {} column => {}", tokenVec.at(_idx).startLine, tokenVec.at(_idx).startColumn);
        LJP_ASSERT(false, "Unexpected token");
    }

    switch (filterKind) {
    case FilterKind::Filter:
        // <retToken> = <tblToken>.filter <leftBracketToken> <refToken> => <bodyStartToken> ... <returnToken> ... <rightBracketToken>
        retToken       = tokenVec.at(_idx - 4);
        tblToken       = tokenVec.at(_idx - 2);
        refToken       = tokenVec.at(_idx + 2);
        bodyStartToken = tokenVec.at(_idx + 5);
        _idx++;
        break;
    case FilterKind::FilterSimple:
        // <retToken> = <tblToken>.filter <leftBracketToken> <funcToken> <rightBracketToken>
        retToken       = tokenVec.at(_idx - 4);
        tblToken       = tokenVec.at(_idx - 2);
        funcToken      = tokenVec.at(_idx + 2);
        bodyStartToken = funcToken;
        _idx++;
        break;
    case FilterKind::FilterZipWithIndex:
        // <retToken> = <tblToken>.filter.zipWithIndex <leftBracketToken> (<refToken>, <idxToken>) => <bodyStartToken> ... <returnToken> ... <rightBracketToken>
        retToken       = tokenVec.at(_idx - 4);
        tblToken       = tokenVec.at(_idx - 2);
        refToken       = tokenVec.at(_idx + 5);
        idxToken       = tokenVec.at(_idx + 7);
        bodyStartToken = tokenVec.at(_idx + 11);
        _idx += 3;
        break;
    case FilterKind::ZipWithIndexFilter:
        // <retToken> = <tblToken>.zipWithIndex.filter <leftBracketToken> (<idxToken>, <refToken>) => <bodyStartToken> ... <returnToken> ... <rightBracketToken>
        retToken       = tokenVec.at(_idx - 6);
        tblToken       = tokenVec.at(_idx - 4);
        refToken       = tokenVec.at(_idx + 5);
        idxToken       = tokenVec.at(_idx + 3);
        bodyStartToken = tokenVec.at(_idx + 9);
        _idx++;
        break;
    default:
        LJP_ASSERT(false, "Unknown filter kind");
    }

    if (processedTokenLines.count(tblToken.startLine) > 0 && processedTokenColumns.count(tblToken.startColumn) > 0) {
        return;
    }

    auto token      = tokenVec.at(_idx);
    auto startToken = token;
    LJP_ASSERT(token.data == "{", "Unexpected token: %s", token.data.c_str());

    while (token.data == "{" || bracketCnt != 0) {
        if (token.data == "}") {
            bracketCnt--;
            if (bracketCnt == 0) {
                break;
            }
        } else if (token.data == "{") {
            bracketCnt++;
            parseFilter(_idx + 1);
        }

        _idx++;
        token = tokenVec.at(_idx);
    }

    // FilterSimple does not have return token
    if (filterKind != FilterKind::FilterSimple) {
        auto tmpIdx = _idx;
        while (tokenVec.at(tmpIdx).kind != TokenKind::Return) {
            tmpIdx--;
            if (tokenVec.at(tmpIdx).idx == tblToken.idx) {
                LJP_ASSERT(false, "Cannot find return token!\n");
            }
        }
        returnToken = tokenVec.at(tmpIdx);
    }
    rightBracketToken = token; // The final matched token is right bracket

    processedTokenLines.insert(tblToken.startLine);
    processedTokenColumns.insert(tblToken.startColumn);

    if (replacedTokenLines.count(tblToken.startLine) > 0 && replacedTokenColumns.count(tblToken.startColumn) > 0) {
        return;
    } else {
        replacedTokenLines.insert(tblToken.startLine);
        replacedTokenColumns.insert(tblToken.startColumn);
    }

    if (tblToken.startLine == bodyStartToken.startLine) {
        if (filterKind == FilterKind::FilterSimple) {
            oldContentLines[rightBracketToken.startLine - 1].replace(rightBracketToken.startColumn, rightBracketToken.startColumn - rightBracketToken.endColumn, ") end end");
            oldContentLines[funcToken.startLine - 1].replace(funcToken.startColumn, funcToken.endColumn - funcToken.startColumn, "if " + funcToken.data + "(" + refToken.data + ") then " + "_tinsert(" + retToken.data + ", " + refToken.data);
        } else {
            oldContentLines[rightBracketToken.startLine - 1].replace(rightBracketToken.startColumn, rightBracketToken.startColumn - rightBracketToken.endColumn, " then _tinsert(" + retToken.data + ", " + refToken.data + ") end end");
            oldContentLines[returnToken.startLine - 1].replace(returnToken.startColumn, returnToken.endColumn - returnToken.startColumn, "if");
        }
        oldContentLines[tblToken.startLine - 1].replace(retToken.startColumn, bodyStartToken.startColumn - retToken.startColumn, retToken.data + " = {}; for " + idxToken.data + ", " + refToken.data + " in ipairs(" + tblToken.data + ") do ");
    } else {
        if (filterKind == FilterKind::FilterSimple) {
            oldContentLines[rightBracketToken.startLine - 1].replace(rightBracketToken.startColumn, rightBracketToken.startColumn - rightBracketToken.endColumn, "end");
            oldContentLines[tblToken.startLine - 1] = oldContentLines[tblToken.startLine - 1].substr(0, retToken.startColumn) + retToken.data + " = {}; for " + idxToken.data + ", " + refToken.data + " in ipairs(" + tblToken.data + ") do ";
            oldContentLines[funcToken.startLine - 1].replace(funcToken.startColumn, funcToken.endColumn - funcToken.startColumn, "if " + funcToken.data + "(" + refToken.data + ") then " + "_tinsert(" + retToken.data + ", " + refToken.data + ") end");
        } else {
            oldContentLines[rightBracketToken.startLine - 1].replace(rightBracketToken.startColumn, rightBracketToken.endColumn - rightBracketToken.startColumn, " then _tinsert(" + retToken.data + ", " + refToken.data + ") end end");
            oldContentLines[tblToken.startLine - 1] = oldContentLines[tblToken.startLine - 1].substr(0, retToken.startColumn) + retToken.data + " = {}; for " + idxToken.data + ", " + refToken.data + " in ipairs(" + tblToken.data + ") do ";
            oldContentLines[returnToken.startLine - 1].replace(returnToken.startColumn, returnToken.endColumn - returnToken.startColumn, "if");
        }
        for (int i = tblToken.startLine + 1; i <= bodyStartToken.startLine; i++) {
            if (i == bodyStartToken.startLine) {
                oldContentLines[i - 1].replace(0, bodyStartToken.startColumn, std::string(bodyStartToken.startColumn, ' '));
            } else {
                oldContentLines[i - 1] = "--[[line keeper]]";
            }
        }
    }
}

void CustomLuaTransformer::parseCompTime(int idx) {
    int bracketCnt = 0;
    int _idx       = idx;

    while (tokenVec.at(_idx).kind != TokenKind::CompTime) {
        _idx++;
        if (tokenVec.at(_idx).kind == TokenKind::EndOfFile) {
            return;
        }
    }

    // compTimeToken [ "(" <compTimeName> ")" ] leftBracketToken <compTimeContent> rightBracketToken
    Token compTimeToken = tokenVec.at(_idx);
    Token compTimeNameOpt;
    Token leftBracketToken;
    Token rightBracketToken;
    if (processedTokenLines.count(compTimeToken.startLine) > 0 && processedTokenColumns.count(compTimeToken.startColumn) > 0) {
        return;
    }

    if (tokenVec.at(_idx + 1).data == "(") {
        compTimeNameOpt = tokenVec.at(_idx + 2);
        LJP_ASSERT(tokenVec.at(_idx + 3).data == ")", "Unexpected token: %s", tokenVec.at(_idx + 3).data.c_str());
        _idx = _idx + 3;
    } else {
        compTimeNameOpt.data = "Unknown";
    }

    _idx++;
    LJP_ASSERT(tokenVec.at(_idx).data == "{", "Unexpected token: %s", tokenVec.at(_idx).data.c_str());
    leftBracketToken = tokenVec.at(_idx);

    _idx++;
    bracketCnt++;

    while (bracketCnt != 0) {
        auto token = tokenVec.at(_idx);

        if (token.data == "{") {
            bracketCnt++;
        } else if (token.data == "}") {
            bracketCnt--;
        }

        _idx++;
    }
    rightBracketToken = tokenVec.at(_idx - 1);
    LJP_ASSERT(rightBracketToken.data == "}", "Unexpected token: %s", rightBracketToken.data.c_str());

    processedTokenLines.insert(compTimeToken.startLine);
    processedTokenColumns.insert(compTimeToken.startColumn);

    std::string compTimeContent = getContentBetween(leftBracketToken, rightBracketToken);
    std::string luaCode         = luaDoString(std::string(filename_ + "/compTime/" + compTimeNameOpt.data + ":" + std::to_string(compTimeToken.startLine)).c_str(), compTimeContent.c_str());

    if (replacedTokenLines.count(compTimeToken.startLine) > 0 && replacedTokenColumns.count(compTimeToken.startColumn) > 0) {
        return;
    } else {
        replacedTokenLines.insert(compTimeToken.startLine);
        replacedTokenColumns.insert(compTimeToken.startColumn);
    }

    for (int i = compTimeToken.startLine; i <= rightBracketToken.startLine; i++) {
        oldContentLines[i - 1] = "--[[line keeper]] ";
    }
    oldContentLines[compTimeToken.startLine - 1] = "--[[comp_time]] ";
    oldContentLines[leftBracketToken.startLine - 1] += luaCode;
}

void CustomLuaTransformer::parseInclude(int idx) {
    int bracketCnt = 0;
    int _idx       = idx;

    while (tokenVec.at(_idx).kind != TokenKind::Include) {
        _idx++;
        if (tokenVec.at(_idx).kind == TokenKind::EndOfFile) {
            return;
        }
    }

    Token includeToken = tokenVec.at(_idx);
    Token leftBracketToken;
    Token rightBracketToken;

    if (processedTokenLines.count(includeToken.startLine) > 0 && processedTokenColumns.count(includeToken.startColumn) > 0) {
        return;
    }

    _idx++;
    leftBracketToken = tokenVec.at(_idx);
    LJP_ASSERT(leftBracketToken.data == "(", "Unexpected token: %s", leftBracketToken.data.c_str());

    _idx++;
    bracketCnt++;
    while (bracketCnt != 0) {
        auto token = tokenVec.at(_idx);

        if (token.data == "(") {
            bracketCnt++;
        } else if (token.data == ")") {
            bracketCnt--;
        }

        _idx++;
    }

    rightBracketToken = tokenVec.at(_idx - 1);
    LJP_ASSERT(rightBracketToken.data == ")", "Unexpected token: %s", rightBracketToken.data.c_str());
    LJP_ASSERT(leftBracketToken.startLine == rightBracketToken.startLine, "leftBracketToken and rightBracketToken are not on the same line");

    processedTokenLines.insert(includeToken.startLine);
    processedTokenColumns.insert(includeToken.startColumn);

    std::string includePackage = getContentBetween(leftBracketToken, rightBracketToken);

    std::string luaCode = std::string("return assert(package.searchpath(") + includePackage + ", package.path))";
    auto includeFile    = luaDoString(std::string(filename_ + "/include" + ":" + std::to_string(includeToken.startLine)).c_str(), luaCode.c_str());

    auto targetFilename = file_transform(includeFile, luaDoString);
    if (targetFilename == nullptr) {
        LJP_WARNING("Unable to open file: %s, check if this file is empty.\n", includeFile);

        if (leftBracketToken.startLine == rightBracketToken.startLine) {
            oldContentLines[leftBracketToken.startLine - 1] = "--[[include file error or empty]]";
        } else {
            for (int i = includeToken.startLine; i <= rightBracketToken.startLine; i++) {
                if (i == includeToken.startLine) {
                    oldContentLines[i - 1] = "--[[include file error or empty]]";
                } else {
                    oldContentLines[i - 1] = "--[[line keeper]]";
                }
            }
        }

        return;
    }
    std::ifstream file(targetFilename);
    std::string includeContent = "";

    if (file.is_open()) {
        std::string line;
        while (std::getline(file, line)) {

            // Regular expressions for Lua comments
            std::regex singleLineComment(R"(--[^\n]*)");
            std::regex multiLineComment(R"(--\[\[[\s\S]*?\]\])");
            std::string result = line;

            // Replace multi-line comments
            auto multiLineBegin = std::sregex_iterator(result.begin(), result.end(), multiLineComment);
            auto multiLineEnd   = std::sregex_iterator();

            for (std::sregex_iterator i = multiLineBegin; i != multiLineEnd; ++i) {
                result.replace(i->position(), i->length(), "");
            }

            // Replace single-line comments
            auto singleLineBegin = std::sregex_iterator(result.begin(), result.end(), singleLineComment);
            auto singleLineEnd   = std::sregex_iterator();

            for (std::sregex_iterator i = singleLineBegin; i != singleLineEnd; ++i) {
                result.replace(i->position(), i->length(), "");
            }

            includeContent += result + " ";
        }

        file.close();
    } else {
        LJP_ASSERT(false, "Unable to open file: %s", targetFilename);
    }

    if (replacedTokenLines.count(includeToken.startLine) > 0 && replacedTokenColumns.count(includeToken.startColumn) > 0) {
        return;
    } else {
        replacedTokenLines.insert(includeToken.startLine);
        replacedTokenColumns.insert(includeToken.startColumn);
    }

    if (leftBracketToken.startLine == rightBracketToken.startLine) {
        oldContentLines[leftBracketToken.startLine - 1] = includeContent;
    } else {
        for (int i = includeToken.startLine; i <= rightBracketToken.startLine; i++) {
            if (i == includeToken.startLine) {
                oldContentLines[i - 1] = includeContent;
            } else {
                oldContentLines[i - 1] = "--[[line keeper]]";
            }
        }
    }

    // std::cout << "[Debug] get Include " << includeContent << std::endl;
}

void CustomLuaTransformer::parse(int idx) {
    int _idx   = idx;
    auto token = tokenVec.at(_idx);
    while (true) {
        // fmt::println("parse {:8} {:8}", token.data, toString(token.kind));

        switch (token.kind) {
        case TokenKind::Foreach:
            parseForeach(_idx);
            break;
        case TokenKind::Map:
            parseMap(_idx);
            break;
        case TokenKind::Filter:
            parseFilter(_idx);
            break;
        case TokenKind::CompTime:
            parseCompTime(_idx);
            break;
        case TokenKind::Include:
            parseInclude(_idx);
            break;
        default:
            break;
        }

        _idx++;
        token = tokenVec.at(_idx);
        if (token.kind == TokenKind::EndOfFile)
            return;
    }
}

void CustomLuaTransformer::dumpContentLines(bool hasLineNumbers) {
    std::cout << "\n\n";
    if (hasLineNumbers) {
        for (size_t i = 0; i < oldContentLines.size(); ++i) {
            std::cout << i + 1 << ": " << oldContentLines[i] << std::endl;
        }
    } else {
        for (size_t i = 0; i < oldContentLines.size(); ++i) {
            std::cout << oldContentLines[i] << std::endl;
        }
    }
    std::cout << "\n\n";
}

} // namespace lua_transformer

// Interface functions for lj_load.c
extern "C" {

using namespace lua_transformer;

const char *file_transform(const char *filename, LuaDoStringPtr func) {
    static std::string proccessedSuffix  = ".1.proccessed";
    static std::string transformedSuffix = ".2.transformed";
    static std::string cacheDir          = LJ_PRO_CACHE_DIR;
    static bool isInit                   = false;
    if (!isInit) {
        isInit = true;

        luaDoString = func;

        {
            const char *value = std::getenv("LJP_NO_PID_DIR");
            if (value != nullptr && strcmp(value, "1") == 0) {
                std::cout << "[luajit-pro] LJP_NO_PID_DIR is enabled" << std::endl;
            } else {
                cacheDir = cacheDir + "/" + std::to_string(getpid());
            }
        }

        if (!std::filesystem::exists(cacheDir)) {
            if (!std::filesystem::create_directories(cacheDir)) {
                LJP_ASSERT(false, "Failed to create folder.");
            }
        }

        {
            const char *value = std::getenv("LJP_KEEP_FILE");
            if (value != nullptr && strcmp(value, "1") == 0) {
                std::cout << "[luajit-pro] LJP_KEEP_FILE is enabled" << std::endl;
            } else {
                std::atexit([]() {
                    for (const auto &file : removeFiles) {
                        // std::cout << "[Debug][file_transform] remove => " << file << std::endl;
                        std::remove(file.c_str());
                    }
                    std::filesystem::remove(cacheDir);
                });
            }
        }

        {
            const char *value = std::getenv("LJP_WITH_PID_SUFFIX");
            if (value != nullptr && strcmp(value, "1") == 0) {
                std::cout << "[luajit-pro] LJP_WITH_PID_SUFFIX is enabled" << std::endl;
                proccessedSuffix  = proccessedSuffix + "." + std::to_string((int)getpid());
                transformedSuffix = transformedSuffix + "." + std::to_string((int)getpid());
            }
        }
    }

    std::ifstream inputFile(filename);
    if (!inputFile) {
        LJP_ASSERT(false, "Cannot open file: %s", filename);
    }

    // std::cout << "[Debug] inputFile => " << filename << std::endl;

    bool disablePreprocess = false;
    std::string firstLine;
    if (std::getline(inputFile, firstLine)) {
        // std::cout << "[Debug] first line => " << firstLine << std::endl;
        std::regex pattern(R"(preprocess:\s*(\w+))"); // You can DISABLE preprocess by adding "preprocess: false" at the first line of the file after the "--[[luajit-pro]]" comment. e.g. "--[[luajit-pro]] preprocess: false"
        std::smatch matches;

        if (std::regex_search(firstLine, matches, pattern)) {
            disablePreprocess = matches[1] == "false";
        }

        if (firstLine.find("--[[luajit-pro]]") == std::string::npos) {
            // std::cout << "[luajit-pro] File: "<< filename << " does not contain the required comment: \"--[[luajit-pro]]\" at the first line." << std::endl;
            return filename;
        }
    } else {
        LJP_WARNING("Cannot read file %s, check if this file is empty.\n", filename);
        return nullptr;
    }
    inputFile.close();

    std::filesystem::path filepath(filename);
    std::string newFileName = cacheDir + "/" + filepath.filename().string();

    std::string proccesedFile = newFileName + proccessedSuffix;
    std::string cppCMD        = "";
    if (disablePreprocess) {
        std::cout << "[luajit-pro] preprocess is disabled in file: " << filename << std::endl;
        cppCMD = std::string("cp ") + filename + " " + proccesedFile;
    } else {
        cppCMD = std::string("cpp ") + filename + " -E | sed '/^#/d' > " + proccesedFile; // `-E`: Preprocess only
    }
    std::system(cppCMD.c_str());
    removeFiles.push_back(proccesedFile);

    // std::ifstream file(proccesedFile);
    // std::string line;
    // while (std::getline(file, line)) {
    //     std::cout << "[Debug] get => " << line << std::endl;
    // }
    // file.close();

    CustomLuaTransformer transformer(proccesedFile);
    transformer.tokenize();
    transformer.parse(0);
    // transformer.dumpContentLines(false);

    auto finalFilePath = newFileName + transformedSuffix;
    removeFiles.push_back(finalFilePath);

    std::ofstream outFile(finalFilePath, std::ios::trunc);
    if (!outFile.is_open()) {
        LJP_ASSERT(false, "Cannot write file: %s", finalFilePath.c_str());
    }

    for (const auto &line : transformer.oldContentLines) {
        outFile << line << std::endl;
    }
    outFile.close();

    char *c_filepath = (char *)malloc(finalFilePath.size() + 1);
    if (c_filepath) {
        std::copy(finalFilePath.begin(), finalFilePath.end(), c_filepath);
        c_filepath[finalFilePath.size()] = '\0'; // Null-terminate
    }

    return c_filepath;
}

void string_transform(const char *str, size_t *output_size) {
    // TODO:
    // std::string inputString(str);
    // std::cout << "[Debug]transformedString => \n>>>\n" << inputString << "\n<<<" << std::endl;
    // *output_size = inputString.size();
    // str = (const char *)inputString.c_str();
}
}
