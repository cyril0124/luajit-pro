# luajit-pro

`luajit-pro` is a LuaJIT fork with some extra syntax. It is based on [the openresty fork of LuaJIT 2.1.0.](https://github.com/openresty/luajit2)

We add an extra syntax transformer on the `lj_load.c` which contains the entry point of the file loader and string loader of LuaJIT. So the original file will be passed into our custom syntax transformer first and the custom syntax will be tansformed into Lua code which can be further parsed and compiled by LuaJIT later(see [lj_load_helper.cpp](patch/src/lj_load_helper.cpp/) and [lib.rs](src/lib.rs) for the detailed implementaion).
