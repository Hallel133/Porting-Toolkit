# Cocos2d-x export (Lua and JavaScript games)

Runs Cocos2d-x games written in **Lua** (`src/main.lua`, `src/cocos/`) or **JavaScript**
(`main.js`, `script/jsb*.js`) on macOS, Windows and Linux with the **Cocos2d-x simulator**.

- Cocos2d-x publishes no prebuilt simulator, and the last release is 4.0 (2019). Build it
  once from a Cocos2d-x checkout of **the game's version** (for example `cocos2d-x-3.17.2`:
  `cocos gen-simulator -p mac` / `-p win32` / `-p linux`) and export with
  `--set runtime=PATH` (the Simulator app, or its folder). The path is remembered.
- Refused, with the reason: compiled JavaScript (`.jsc`) and encrypted Lua (`.luac` with an
  XXTEA sign). Only the game's own executable has the key.
- Games that add their own C++ code (custom bindings) start, but the parts that call that
  code fail. C++-only Cocos2d-x games need the source; Cocos2d-html5 and Cocos Creator web
  builds are exported by the HTML5 plugins.

The simulator is MIT-licensed; every build carries `LICENSE-cocos2dx-simulator.txt`.

Not tested with a real simulator or game yet: Porting Toolkit's tests use a fake runtime.
