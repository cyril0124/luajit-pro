/*
** Load and dump code.
** Copyright (C) 2005-2023 Mike Pall. See Copyright Notice in luajit.h
*/

#include <errno.h>
#include <stdio.h>

#define lj_load_c
#define LUA_CORE

#include "lua.h"
#include "lauxlib.h"

#include "lj_obj.h"
#include "lj_gc.h"
#include "lj_err.h"
#include "lj_buf.h"
#include "lj_func.h"
#include "lj_frame.h"
#include "lj_vm.h"
#include "lj_lex.h"
#include "lj_bcdump.h"
#include "lj_parse.h"

#ifdef LUAJIT_SYNTAX_EXTEND
#include <assert.h>
#include <stdlib.h>

#define PURPLE_COLOR "\033[35m"
#define RESET_COLOR "\033[0m"

#define LJP_INFO(fmt, ...)                                                                                                                                                                                                                                                                                                                                                                                         \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        fprintf(stdout, PURPLE_COLOR " [INFO] " RESET_COLOR fmt, __VA_ARGS__);                                                                                                                                                                                                                                                                                                                                 \
        fflush(stdout);                                                                                                                                                                                                                                                                                                                                                                                        \
    } while (0)

#define LJP_WARNING(fmt, ...)                                                                                                                                                                                                                                                                                                                                                                                        \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        fprintf(stdout, "[%s:%s:%d]" PURPLE_COLOR " [WARNING] " RESET_COLOR fmt, __FILE__, __func__, __LINE__, __VA_ARGS__);                                                                                                                                                                                                                                                                                     \
        fflush(stdout);                                                                                                                                                                                                                                                                                                                                                                                        \
    } while (0)

#define LJP_DEBUG(fmt, ...)                                                                                                                                                                                                                                                                                                                                                                                        \
    do {                                                                                                                                                                                                                                                                                                                                                                                                       \
        fprintf(stdout, "[%s:%s:%d]" PURPLE_COLOR " [DEBUG] " RESET_COLOR fmt, __FILE__, __func__, __LINE__, __VA_ARGS__);                                                                                                                                                                                                                                                                                     \
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

char *ljp_file_transform(const char *filename);
void ljp_string_transform(const char *str, size_t *output_size);
void luaL_openlibs(lua_State *L);

void ljp_string_file_reset_ptr(const char *filename);
size_t ljp_string_file_get_content(char *buf, size_t expectSize, const char *filename);
char ljp_string_file_check_eof(const char *filename);

#endif // LUAJIT_SYNTAX_EXTEND

/* -- Load Lua source code and bytecode ----------------------------------- */

static TValue *cpparser(lua_State *L, lua_CFunction dummy, void *ud)
{
  LexState *ls = (LexState *)ud;
  GCproto *pt;
  GCfunc *fn;
  int bc;
  UNUSED(dummy);
  cframe_errfunc(L->cframe) = -1;  /* Inherit error function. */
  bc = lj_lex_setup(L, ls);
  if (ls->mode) {
    int xmode = 1;
    const char *mode = ls->mode;
    char c;
    while ((c = *mode++)) {
      if (c == (bc ? 'b' : 't')) xmode = 0;
      if (c == (LJ_FR2 ? 'W' : 'X')) ls->fr2 = !LJ_FR2;
    }
    if (xmode) {
      setstrV(L, L->top++, lj_err_str(L, LJ_ERR_XMODE));
      lj_err_throw(L, LUA_ERRSYNTAX);
    }
  }
  pt = bc ? lj_bcread(ls) : lj_parse(ls);
  if (ls->fr2 == LJ_FR2) {
    fn = lj_func_newL_empty(L, pt, tabref(L->env));
    /* Don't combine above/below into one statement. */
    setfuncV(L, L->top++, fn);
  } else {
    /* Non-native generation returns a dumpable, but non-runnable prototype. */
    setprotoV(L, L->top++, pt);
  }
  return NULL;
}

LUA_API int lua_loadx(lua_State *L, lua_Reader reader, void *data,
		      const char *chunkname, const char *mode)
{
  LexState ls;
  int status;
  ls.rfunc = reader;
  ls.rdata = data;
  ls.chunkarg = chunkname ? chunkname : "?";
  ls.mode = mode;
  lj_buf_init(L, &ls.sb);
  status = lj_vm_cpcall(L, NULL, &ls, cpparser);
  lj_lex_cleanup(L, &ls);
  lj_gc_check(L);
  return status;
}

LUA_API int lua_load(lua_State *L, lua_Reader reader, void *data,
		     const char *chunkname)
{
  return lua_loadx(L, reader, data, chunkname, NULL);
}

typedef struct FileReaderCtx {
#ifdef LUAJIT_SYNTAX_EXTEND
  char filename[256]; /* Max 255 + 1 for null terminator. */
  unsigned char is_first_access;
  unsigned char transformed;
#endif // LUAJIT_SYNTAX_EXTEND
  FILE *fp;
  char buf[LUAL_BUFFERSIZE];
} FileReaderCtx;

static const char *reader_file(lua_State *L, void *ud, size_t *size)
{
  FileReaderCtx *ctx = (FileReaderCtx *)ud;
  UNUSED(L);

#ifdef LUAJIT_SYNTAX_EXTEND
  if(ctx->transformed) {
    if (ljp_string_file_check_eof(ctx->filename)) return NULL;
  } else {
    if (feof(ctx->fp)) return NULL;
  }
#else
  if (feof(ctx->fp)) return NULL;
#endif

#ifdef LUAJIT_SYNTAX_EXTEND
  if(ctx->is_first_access == 1) {
    // The file read by LuaJIT is seperated by many parts to avoid stack overflow for some large files.
    ctx->is_first_access = 0;

    static char first_line_buffer[256];
    static const char* substring = "luajit-pro";

    if (fgets(first_line_buffer, sizeof(first_line_buffer), ctx->fp) != NULL) {
      if (strstr(first_line_buffer, substring) != NULL) {
        char *new_file = ljp_file_transform(ctx->filename);
        // printf("[Debug]new_file => %s\n", new_file);fflush(stdout);
        if(new_file == NULL) {
          LJP_WARNING("new_file is NULL! Cannot read file: %s, check if this file is empty. errno: %s\n", ctx->filename, strerror(errno));
        } else {
          ctx->transformed = 1;
          ljp_string_file_reset_ptr(ctx->filename);
        }
      } else {
        // The read file did not contains "--[[luajit-pro]]"
      }
      fseek(ctx->fp, 0, SEEK_SET);
    } else {
      LJP_WARNING("Cannot read file: %s, check if this file is empty. errno: %s\n", ctx->filename, strerror(errno));
    }
  }

  if(ctx->transformed == 1) {
    *size = ljp_string_file_get_content(ctx->buf, sizeof(ctx->buf), ctx->filename);
  } else {
    *size = fread(ctx->buf, 1, sizeof(ctx->buf), ctx->fp);
  }
#else
  *size = fread(ctx->buf, 1, sizeof(ctx->buf), ctx->fp);
#endif // LUAJIT_SYNTAX_EXTEND

  return *size > 0 ? ctx->buf : NULL;
}

LUALIB_API int luaL_loadfilex(lua_State *L, const char *filename,
			      const char *mode)
{
#ifdef LUAJIT_SYNTAX_EXTEND
  static char init = 0;
  if(init == 0) {
    init = 1;
    const char *code_str =
"local function search_for(module_name, suffix, path, tried)\n"
"   for entry in path:gmatch(\"[^;]+\") do\n"
"      local slash_name = module_name:gsub(\"%.\", \"/\")\n"
"      local filename = entry:gsub(\"?\", slash_name)\n"
"      local source_filename = filename:gsub(\"%\.lua$\", suffix)\n"
"      local fd = io.open(source_filename, \"rb\")\n"
"      if fd then\n"
"         return source_filename, fd, tried\n"
"      end\n"
"      table.insert(tried, \"no file '\" .. source_filename .. \"'\")\n"
"   end\n"
"   return nil, nil, tried\n"
"end\n"
"\n"
"local function search_module(module_name, search_dtl)\n"
"   local found\n"
"   local fd\n"
"   local tried = {}\n"
"   local path = os.getenv(\"LUA_PATH\") or package.path\n"
"   found, fd, tried = search_for(module_name, \".lua\", path, tried)\n"
"   if found then\n"
"      return found, fd\n"
"   end\n"
"   local tl_path = os.getenv(\"TL_PATH\") or package.path\n"
"   if search_dtl then\n"
"      found, fd, tried = search_for(module_name, \".d.tl\", tl_path, tried)\n"
"      if found then\n"
"         return found, fd\n"
"      end\n"
"   end\n"
"   found, fd, tried = search_for(module_name, \".tl\", tl_path, tried)\n"
"   if found then\n"
"      return found, fd\n"
"   end\n"
"   local luau_path = os.getenv(\"LUAU_PATH\") or package.path\n"
"   found, fd, tried = search_for(module_name, \".luau\", luau_path, tried)\n"
"   if found then\n"
"      return found, fd\n"
"   end\n"
"   return nil, nil, tried\n"
"end\n"
"\n"
"local function ljp_package_loader(module_name)\n"
"   local found_filename, fd, tried = search_module(module_name, true)\n"
"   if found_filename then\n"
"      fd:close()\n"
"      local chunk, err = loadfile(found_filename)\n"
"      if chunk then\n"
"         return function(modname, loader_data)\n"
"            if loader_data == nil then\n"
"               loader_data = found_filename\n"
"            end\n"
"            local ret = chunk(modname, loader_data)\n"
"            package.loaded[module_name] = ret\n"
"            return ret\n"
"         end, found_filename\n"
"      else\n"
"         error(\"Internal Compiler Error: luajit-pro produced invalid Lua.\\n\\n\" .. err)\n"
"      end\n"
"   end\n"
"   return table.concat(tried, \"\\n\\t\")\n"
"end\n"
"\n"
"if _G.package.searchers then\n"
"   table.insert(_G.package.searchers, 2, ljp_package_loader)\n"
"else\n"
"   table.insert(_G.package.loaders, 2, ljp_package_loader)\n"
"end\n";
    if (luaL_dostring(L, code_str) != LUA_OK) {
      // If execution fails, get the error message
      const char *err_msg = lua_tostring(L, -1);
      printf("Error executing Lua code: %s\n", err_msg);

      // Clean up the stack by popping the error message
      lua_pop(L, 1); // Remove the error message from the stack
      lua_close(L); // Close the Lua state
      assert(0 && "Error executing initialize luaCode");
    }
  }
#endif

  FileReaderCtx ctx;
  int status;
  const char *chunkname;
  if (filename) {
    ctx.fp = fopen(filename, "rb");
    if (ctx.fp == NULL) {
      lua_pushfstring(L, "cannot open %s: %s", filename, strerror(errno));
      return LUA_ERRFILE;
    }
    chunkname = lua_pushfstring(L, "@%s", filename);
  } else {
    ctx.fp = stdin;
    chunkname = "=stdin";
  }

#ifdef LUAJIT_SYNTAX_EXTEND
  // Save filename to ctx
  if(filename == NULL) {
    lua_pushfstring(L, "filename is nil");
    return LUA_ERRFILE;
  } else {
    snprintf(ctx.filename, sizeof(ctx.filename), "%s", filename);
  }

  // A flag that indicates whether it is the first access to the file.
  ctx.is_first_access = 1;
  ctx.transformed = 0;
#endif // LUAJIT_SYNTAX_EXTEND

  status = lua_loadx(L, reader_file, &ctx, chunkname, mode);
  if (ferror(ctx.fp)) {
    L->top -= filename ? 2 : 1;
    lua_pushfstring(L, "cannot read %s: %s", chunkname+1, strerror(errno));
    if (filename)
      fclose(ctx.fp);
    return LUA_ERRFILE;
  }
  if (filename) {
    L->top--;
    copyTV(L, L->top-1, L->top);
    fclose(ctx.fp);
  }
  return status;
}

LUALIB_API int luaL_loadfile(lua_State *L, const char *filename)
{
  return luaL_loadfilex(L, filename, NULL);
}

typedef struct StringReaderCtx {
  const char *str;
  size_t size;
} StringReaderCtx;

static const char *reader_string(lua_State *L, void *ud, size_t *size)
{
  StringReaderCtx *ctx = (StringReaderCtx *)ud;
  UNUSED(L);
  if (ctx->size == 0) return NULL;
  *size = ctx->size;
  ctx->size = 0;

#ifdef LUAJIT_SYNTAX_EXTEND
  ljp_string_transform(ctx->str, size);
#endif // LUAJIT_SYNTAX_EXTEND

  return ctx->str;
}

LUALIB_API int luaL_loadbufferx(lua_State *L, const char *buf, size_t size,
				const char *name, const char *mode)
{
  StringReaderCtx ctx;
  ctx.str = buf;
  ctx.size = size;
  return lua_loadx(L, reader_string, &ctx, name, mode);
}

LUALIB_API int luaL_loadbuffer(lua_State *L, const char *buf, size_t size,
			       const char *name)
{
  return luaL_loadbufferx(L, buf, size, name, NULL);
}

LUALIB_API int luaL_loadstring(lua_State *L, const char *s)
{
  return luaL_loadbuffer(L, s, strlen(s), s);
}

/* -- Dump bytecode ------------------------------------------------------- */

LUA_API int lua_dump(lua_State *L, lua_Writer writer, void *data)
{
  cTValue *o = L->top-1;
  uint32_t flags = LJ_FR2*BCDUMP_F_FR2;  /* Default mode for legacy C API. */
  lj_checkapi(L->top > L->base, "top slot empty");
  if (tvisfunc(o) && isluafunc(funcV(o)))
    return lj_bcwrite(L, funcproto(funcV(o)), writer, data, flags);
  else
    return 1;
}

