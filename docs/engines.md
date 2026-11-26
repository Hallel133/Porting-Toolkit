# Engines

Generated from `engine/ptk_engine/data/engines.json` by `tools/gen_engines_doc.py`; do not edit by hand.

Porting Toolkit recognises 186 engines from their files' names and first bytes (nothing is decoded). `ptk engines` lists them; `ptk scan` says which one a game uses.

## Exported by built-in plugins (6)

| Engine | Detected | How |
|---|---|---|
| Godot | yes | Scripts and assets live in the .pck (or inside the .exe). The same .pck runs with the Godot export template of the same version for the target. |
| Ren'Py | yes | The game/ folder (.rpa, .rpyc) runs on any platform with the Ren'Py runtime of the same version. |
| RPG Maker MV/MZ | yes | MV/MZ games are HTML5 + JavaScript: the web folder runs in a browser, in a WebKit app on macOS, or with NW.js on Windows and Linux. |
| RPG Maker XP/VX/VX Ace | yes | The game's Ruby scripts and data run on mkxp-z, an open-source RGSS runtime for macOS and Linux. Windows keeps the original Game.exe. Games that use the RTP need it installed. |
| LÖVE | yes | The game is a .love archive of Lua scripts and assets (on its own or fused into the .exe). It runs with the official LÖVE build of the same version for each platform. |
| HTML5 (Construct, GDevelop, Twine, NW.js, Electron) | yes | The game is a web page: it runs in a browser, in a WebKit app on macOS, or with NW.js on Windows and Linux. Electron games that call Node.js need those parts changed. |

## Web engines (exported by the HTML5 plugins) (12)

| Engine | Detected | How |
|---|---|---|
| Construct | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| GDevelop | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| Phaser | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| Twine | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| Three.js | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| Babylon.js | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| PlayCanvas | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| A-Frame (VR) | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| LayaAir | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| Blend4Web | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| Turbulenz | as `html5` | Web games: the HTML5 plugins export them (browser, WebKit app on macOS, NW.js on Windows and Linux). |
| GameSalad | as `html5` | GameSalad's HTML5 export (js/gse/gse-export.js) is a web game: the HTML5 plugins export it. Its native builds have no documented layout. |

## Depends on the build (19)

| Engine | Detected | How |
|---|---|---|
| Unity | yes | Mono builds keep the game's code in Data/Managed/*.dll, which runs with the Unity player of the same version for another platform (found in a Unity install, or downloaded once from Unity: only the player, no Unity install needed). The build's shaders must cover the target's graphics API, and native plugins need their own builds. IL2CPP builds are native code and need the source. |
| GameMaker | yes | data.win holds the game; it needs a GameMaker runner for the target. Compatibility varies by version. |
| MonoGame | yes | MonoGame games are .NET. DesktopGL builds run on macOS and Linux with the same .NET (or Mono) runtime and the target's native libraries (SDL2, OpenAL); WindowsDX builds use DirectX and need the source. |
| FNA | yes | The game already uses FNA, which runs on macOS and Linux. Such games usually ship .bin.osx / .bin.x86_64 launchers (MonoKickstart) and fnalibs: the developer's own Mac and Linux builds are the way to go. |
| Ardor3D | yes | Java: the game's .jar files run on any platform with a Java runtime, as long as the build ships the native libraries (LWJGL, JOGL...) for the target. |
| Java 3D | yes | Java: the game's .jar files run on any platform with a Java runtime, as long as the build ships the native libraries (LWJGL, JOGL...) for the target. |
| jMonkeyEngine | yes | Java: the game's .jar files run on any platform with a Java runtime, as long as the build ships the native libraries (LWJGL, JOGL...) for the target. |
| libGDX | yes | Java: the game's .jar files run on any platform with a Java runtime, as long as the build ships the native libraries (LWJGL, JOGL...) for the target. |
| PlayN | yes | Java: the game's .jar files run on any platform with a Java runtime, as long as the build ships the native libraries (LWJGL, JOGL...) for the target. |
| Kivy | yes | Kivy apps are Python: the app's own scripts run with Kivy for the target, if the build keeps them readable. |
| Pygame | yes | Pygame games are Python: the scripts run with Python and pygame on any platform, if the build keeps them readable. |
| Panda3D | yes | Panda3D games are usually Python with .bam/.egg models; they run with Panda3D for the target if the scripts are readable. |
| PICO-8 | yes | PICO-8 carts (.p8, .p8.png) run in PICO-8 itself (a paid app for every platform) or as its web export. |
| Solar2D (Corona) | yes | Solar2D compiles Lua into resource.car; the open source Solar2D can build the project again for another platform from the source project. |
| AppGameKit | yes | Tier 1 games are bytecode (media/bytecode.byc) for AppGameKit's player, which comes with an AppGameKit licence. |
| Game Editor | yes | Game Editor is open source; a project (.ged) can be exported again for another platform with Game Editor itself. |
| Cocos2d-x (C++) | yes | Cocos2d-x games written in C++ are native code and need the source. Lua and JavaScript games are detected separately; HTML5 builds (Cocos2d-html5, Cocos Creator web) are exported by the HTML5 plugins. |
| Cocos Creator | yes | Web builds are exported by the HTML5 plugins. Native builds embed a customised engine and usually encrypted scripts (.jsc): a port needs the Cocos Creator project. |
| Pixel Game Maker MV | yes | Games are data (Resources/data/project.json) for the Pixel Game Maker MV player, which runs on Windows only. The player is open source (MIT, built on Cocos2d-x 3.17.1: github.com/GGGweb-adm/PGMMV) but has no macOS or Linux build yet. |

## Open-source runtimes (46)

| Engine | Runtime | Plugin | How |
|---|---|---|---|
| SCUMM (LucasArts) | [ScummVM](https://www.scummvm.org) | `scummvm-export` | The game's data files run on ScummVM, which reimplements SCUMM for every platform. |
| Adventure Game Interpreter (Sierra AGI) | [ScummVM](https://www.scummvm.org) | `scummvm-export` | The game's resource files (LOGDIR, PICDIR, VOL.*) run on ScummVM. |
| Adventure Game Studio | [ScummVM](https://www.scummvm.org) | `scummvm-export` | AGS games keep their data in ac2game.dat, a .ags file or appended to the .exe; ScummVM runs them on every platform. |
| Wintermute Engine | [ScummVM](https://www.scummvm.org) | `scummvm-export` | Wintermute games keep their data in .dcp packages, which ScummVM runs on every platform. |
| Freescape (Incentive Software) | [ScummVM](https://www.scummvm.org) | `scummvm-export` | ScummVM runs the DOS releases of Driller, Dark Side, Total Eclipse and Castle Master. |
| Doom engine (id Tech 1) | [GZDoom / UZDoom](https://github.com/UZDoom/UZDoom) | `doom-export` | The game's IWAD (DOOM.WAD, DOOM2.WAD, HERETIC.WAD...) runs on a Doom source port for every platform. |
| Quake engine | [QuakeSpasm / vkQuake](https://github.com/Novum/vkQuake) | `quake-export` | Quake's id1/ folder (pak0.pak, pak1.pak) runs on QuakeSpasm or vkQuake. |
| id Tech 2 (Quake II) | [Yamagi Quake II](https://github.com/yquake2/yquake2) | `quake2-export` | Quake II's baseq2/ folder runs on Yamagi Quake II. |
| Jake2 | [Yamagi Quake II](https://github.com/yquake2/yquake2) | none yet | Jake2 is a Java port of Quake II. Quake II's own data (baseq2/) runs on Yamagi Quake II with the quake2-export plugin. |
| id Tech 3 (Quake III Arena) | [ioquake3](https://github.com/ioquake/ioq3) | `quake3-export` | Quake III Arena's baseq3/ folder runs on ioquake3. Other id Tech 3 games need their own source ports. |
| ioquake3 | [ioquake3](https://github.com/ioquake/ioq3) | none yet | ioquake3 is itself the open source port of id Tech 3. Quake III Arena's baseq3/ data runs on it with the quake3-export plugin. |
| id Tech 4 (Doom 3) | [dhewm3](https://github.com/dhewm/dhewm3) | `doom3-export` | Doom 3 and Resurrection of Evil (base/*.pk4) run on dhewm3. Quake 4 and Prey are not supported by it. |
| Wolfenstein 3D engine | [ECWolf](https://bitbucket.org/ecwolf/ecwolf) | `wolf3d-export` | The game's data files (VSWAP.WL6, GAMEMAPS.WL6...) run on ECWolf. |
| Build engine | [Raze](https://github.com/ZDoom/Raze) | `build-export` | Duke Nukem 3D, Blood, Shadow Warrior and other Build games run on Raze with their original data. |
| Jedi engine (Dark Forces) | [The Force Engine](https://github.com/luciusDXL/TheForceEngine) | `jedi-export` | Dark Forces' .GOB files run on The Force Engine. |
| GoldSrc (Half-Life engine) | [Xash3D FWGS](https://github.com/FWGS/xash3d-fwgs) | `goldsrc-export` | Half-Life's valve/ folder runs on Xash3D FWGS (the game's own DLLs need builds for the target; Valve's are Windows only). |
| Infinity Engine | [GemRB](https://github.com/gemrb/gemrb) | `infinity-export` | Baldur's Gate, Icewind Dale and Planescape: Torment data (chitin.key, .bif) run on GemRB. |
| OpenMW (Morrowind) | [OpenMW](https://openmw.org) | `morrowind-export` | Morrowind's Data Files (Morrowind.esm, .bsa) run on OpenMW. |
| Exult (Ultima VII) | [Exult](https://github.com/exult/exult) | `exult-export` | Ultima VII's data (STATIC/U7CHUNKS...) runs on Exult. |
| XnGine (Daggerfall) | [Daggerfall Unity](https://github.com/Interkarma/daggerfall-unity) | `daggerfall-export` | The Elder Scrolls II: Daggerfall's ARENA2 folder runs on Daggerfall Unity. Other XnGine games have no open runtime. |
| Stratagus | [Stratagus](https://github.com/Wargus/stratagus) | `stratagus-export` | Stratagus games (Wargus, Stargus...) are Lua data for the open source Stratagus engine. Wargus needs data extracted from the original Warcraft II. |
| NScripter | [ONScripter](https://onscripter.osdn.jp) | `nscripter-export` | NScripter novels (nscript.dat, .nsa archives) run on ONScripter. |
| OHRRPGCE | [OHRRPGCE](https://rpg.hamsterrepublic.com/ohrrpgce/) | `ohrrpgce-export` | The game's .rpg file runs on the OHRRPGCE player (open source) for every platform. |
| M.U.G.E.N | [Ikemen GO](https://github.com/ikemen-engine/Ikemen-GO) | `mugen-export` | M.U.G.E.N content (chars/, stages/, data/mugen.cfg) runs on Ikemen GO. |
| Aleph One (Marathon) | [Aleph One](https://alephone.lhowon.org) | `alephone-export` | Marathon's Shapes, Sounds and Map files run on Aleph One. |
| FreeSpace 2 (Source Code Project) | [FreeSpace Open](https://github.com/scp-fs2open/fs2open.github.com) | `fs2-export` | FreeSpace 2's .vp archives run on FreeSpace Open. |
| RPG Maker 2000/2003 | [EasyRPG Player](https://easyrpg.org) | `rpgmaker2k-export` | RPG_RT games run on EasyRPG Player. |
| XNA (Microsoft XNA Framework 4) | [FNA](https://fna-xna.github.io) | `fna-export` | XNA games are .NET with XNA's content (.xnb). They run unchanged on macOS and Linux with FNA, its XNA assemblies, fnalibs and MonoKickstart: none has a prebuilt official release, so the fna-export plugin uses an FNA kit you build once. |
| Blender Game Engine | [UPBGE](https://upbge.org) | none yet | UPBGE, the community continuation of the Blender Game Engine, can run many .blend games; older files may need changes. No Porting Toolkit plugin for it yet. UPBGE publishes only weekly builds, which cannot be pinned, so there is no plugin yet. |
| Adobe Flash (SWF) | [Ruffle](https://ruffle.rs) | `flash-export` | Flash games run in Ruffle (web or desktop). Games built on Stage3D (Starling, Flare3D, Away3D) are not supported by Ruffle yet. |
| Flixel | [Ruffle](https://ruffle.rs) | none yet | Flixel games are Flash (.swf): the flash-export plugin runs them with Ruffle. HaxeFlixel builds are native or HTML5. |
| Scratch | [TurboWarp Packager](https://packager.turbowarp.org) | `scratch-export` | Scratch projects (.sb3) become web or desktop apps with the TurboWarp Packager. |
| TIC-80 | [TIC-80](https://github.com/nesbox/TIC-80) | `tic80-export` | TIC-80 carts (.tic) run on the open source TIC-80 for every platform. |
| Pyrogenesis (0 A.D.) | [0 A.D. / Pyrogenesis](https://play0ad.com) | none yet | 0 A.D. is open source and released for every platform; mods are data for it. |
| OpenClonk | [OpenClonk](https://www.openclonk.org) | none yet | OpenClonk is open source for every platform; scenarios are data for it. |
| Qfusion (Warsow) | [Qfusion / Warsow](https://github.com/Qfusion/qfusion) | none yet | Qfusion and Warsow are open source; the basewsw/ data runs on them. |
| Cube Engine | [Cube](http://cubeengine.com) | none yet | Cube is open source; the packages/ folder runs with Cube built for the target. |
| Cube 2 Engine (Sauerbraten) | [Cube 2: Sauerbraten](http://sauerbraten.org) | none yet | Cube 2: Sauerbraten is open source and released for every platform; the packages/ folder is data for it. |
| StepMania | [ITGmania / StepMania](https://github.com/itgmania/itgmania) | none yet | Songs (.sm, .ssc) play in ITGmania or StepMania on every platform. No plugin yet: ITGmania for Windows is an installer only. |
| Visual Pinball | [Visual Pinball X](https://github.com/vpinball/vpinball) | none yet | Visual Pinball X is open source, with standalone builds for macOS and Linux; tables (.vpx) are data for it. No plugin yet: tables usually also need VPinMAME ROMs and backglass files. |
| Serious Engine | [Serious Sam Classic (Serious Engine 1)](https://github.com/tx00100xt/SeriousSamClassic) | none yet | Serious Engine 1 (The First and Second Encounter) is open source; its .gro data runs on SeriousSamClassic. Later Serious Engine versions need the source. No plugin yet: its Windows builds are 7z archives without checksums. |
| HPL Engine (Frictional Games) | [HPL engines (GPL source releases)](https://github.com/FrictionalGames) | none yet | Penumbra and Amnesia already ship for macOS and Linux, and their HPL engines are open source (GPL). No Porting Toolkit plugin for it yet. |
| Gold Box (SSI) | [DOSBox Staging](https://www.dosbox-staging.org) | `dos-export` | Gold Box games are DOS programs (.DAX data): they run in DOSBox. |
| Defold | [Defold engine](https://github.com/defold/defold) | `defold-export` | Defold's game archive (game.arcd, game.projectc) runs with the Defold engine of the exact same version, which Defold publishes for every platform; the plugin reads the version from the game's own engine. |
| Cocos2d-x (Lua) | [Cocos2d-x simulator (same version as the game)](https://github.com/cocos2d/cocos2d-x) | `cocos2dx-export` | The game's Lua scripts (src/) and resources (res/) run with the Cocos2d-x simulator of the same version, built once from Cocos2d-x's source. Encrypted scripts (XXTEA) and games with their own C++ code run only in the game's own executable. |
| Cocos2d-x (JavaScript) | [Cocos2d-x simulator (same version as the game)](https://github.com/cocos2d/cocos2d-x) | `cocos2dx-export` | The game's JavaScript (main.js, src/) and resources run with the Cocos2d-x simulator of the same version. Compiled .jsc scripts are encrypted and run only in the game's own executable. |

## Needs the source code (77)

| Engine | Detected | How |
|---|---|---|
| Unreal Engine | yes | The game code is compiled into the executable. A native port needs the project source; otherwise run the Windows build through a compatibility layer. |
| RAGE (Rockstar Advanced Game Engine) | yes | Native engine code. Without source, only a compatibility layer (or a hosted/streamed build) can run it elsewhere. |
| Source | yes | Native engine code. Without source, only a compatibility layer can run it elsewhere. |
| Starling Framework | as `flash` | Starling draws with Flash's Stage3D, which Ruffle does not support yet, so there is no open runtime for it today. |
| Flare3D | as `flash` | Flare3D draws with Flash's Stage3D, which Ruffle does not support yet, so there is no open runtime for it today. |
| 4A Engine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Amazon Lumberyard | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Anvil (Scimitar, AnvilNext) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Chrome Engine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| ClanLib | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Clausewitz (Paradox) | yes | Native engine code. Most Paradox games are already sold for macOS and Linux. |
| Clickteam Fusion | yes | Fusion games run on Clickteam's own runtimes; there is no open runtime, so a port needs the .mfa project. |
| Cobra Engine (Frontier) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Codea | yes | Codea projects run only inside the Codea app on iPad. |
| Creation Engine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Gamebryo | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| CryEngine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| O3DE (Open 3D Engine) | yes | O3DE is open source, but the game's code is compiled C++: a port needs the project, built for the target. |
| Crystal Tools | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Crystal Space | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Dagor Engine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Dark Engine (Thief, System Shock 2) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Decima | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Delta3D | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Divinity Engine (Larian) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| DX Studio | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Dunia Engine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| EGO (Codemasters) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Electron (Obsidian, Neverwinter Nights 2) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Aurora (BioWare, Neverwinter Nights) | yes | Neverwinter Nights: Enhanced Edition is already sold for macOS and Linux; xoreos, an open reimplementation, is not playable yet. |
| Odyssey Engine (BioWare) | yes | Native engine code. Knights of the Old Republic is sold for macOS; xoreos is not playable yet. |
| Enforce (Bohemia Interactive) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Essence Engine (Relic) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| ForgeLight | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Fox Engine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Frostbite | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Future Pinball | yes | Future Pinball is a closed Windows program; tables (.fpt) need it. |
| Gamestudio (Acknex) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Genie Engine (Age of Empires) | yes | Native engine code; openage, an open reimplementation, is not playable yet. |
| Horde3D | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Irrlicht | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| IW Engine (Infinity Ward) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Jade (Ubisoft) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| LithTech | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Luminous Engine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Marmalade | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| MT Framework (Capcom) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Northlight (Remedy) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| OGRE | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| ORX | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| PhyreEngine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| REDengine (CD Projekt Red) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| RenderWare | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| S&box | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Source 2 | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| SAGE (EA) | yes | Native engine code. EA published the source of Command & Conquer: Generals in 2025 (GPL); community ports build on it, but Porting Toolkit has no plugin. |
| Snowdrop | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Autodesk Stingray (Bitsquid) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Stride (Xenko) | yes | Stride builds target Windows; its runtime for other platforms needs the project. |
| Telltale Tool | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| TOSHI (Blue Tongue) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Torque3D | yes | Torque3D is open source, but each game ships its own engine build: a port needs the game's engine source. |
| UbiArt Framework | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Unigine | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Vicarious Visions Alchemy | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Virtools | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Vision (Havok) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| VRAGE (Keen Software) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Zillions of Games | yes | Zillions is a closed Windows program; rule files (.zrf) need it. |
| Zero (Pandemic Studios) | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| id Tech 5 | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| id Tech 7 | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |
| Vengeance Engine | as `unreal` | A modified Unreal Engine 2 (Tribes: Vengeance, BioShock): native code, detected as Unreal Engine. |
| Kinetica | no | A PlayStation 2 engine: no PC builds to port. |
| Roblox | no | Roblox games live on Roblox's servers and run only in the Roblox app; there is no game build to port. |
| id Tech 6 | yes | Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. Detected by base/gameresources.index next to its .resources archive (Doom 2016, Wolfenstein II). |
| LyN | as `jade` | LyN games keep their data in Ubisoft's BigFile (.bf), the same format as Jade, so they are detected as Jade. Native engine code: a port needs the game's source code. Without it, only a compatibility layer (running the Windows build through Wine/CrossOver on macOS) can run it elsewhere. |

## Nothing to port (6)

| Engine | Detected | How |
|---|---|---|
| iMUSE | no | iMUSE is LucasArts' interactive music system inside SCUMM games; ScummVM plays it as part of those games. |
| World Builder | no | A map/adventure editor, not a runtime: the games it makes are handled by their engine. |
| OpenSimulator | no | A virtual world server: worlds are hosted and joined with a viewer, not ported. |
| HeroEngine | no | An online engine: games run on their publisher's servers. |
| WorldForge | no | An open source online world framework: worlds run on servers, joined with its client. |
| DimensioneX Multiplayer Engine | no | A server-side engine: games run on a web server and are played in a browser. |

## Not recognised yet (20)

| Engine | Detected | How |
|---|---|---|
| Buildbox | no | Buildbox's export formats are not publicly documented, and no file name or header unique to its games was found. |
| C4 Engine | no | The C4 Engine wiki documents its pack files, but no file name or header unique to shipped C4 games could be confirmed. |
| CraftStudio | no | Only the editor's project format (Project.dat) is documented, by the community; exported games' files are not. CraftStudio is discontinued. |
| Coretech | no | No public documentation of this engine's game files was found. |
| Dim3 | no | dim3 is open source and its games are self-contained XML data folders, but no file name unique to them could be confirmed. |
| Enigma Engine | no | ENIGMA compiles games into native executables with no separate data file, so nothing identifies them. |
| id Tech 8 | no | id Tech 8 (Doom: The Dark Ages) has no publicly documented file layout yet. Indiana Jones and the Great Circle runs on id Tech 7. |
| Iron Engine | no | No public documentation of this engine's game files was found. |
| Leadwerks | no | Published Leadwerks games keep their assets in an encrypted data.zip, a name too common to identify the engine. |
| Messiah | no | No public documentation of this engine's game files was found. |
| Moai SDK | no | Moai games are Lua scripts (main.lua) run by a Moai host; neither is unique to Moai. |
| Monkey X | no | Monkey X desktop builds are an executable plus a data/ folder, and HTML5 builds a generic main.js; nothing is unique to Monkey X. |
| Panta Rhei | no | Capcom's internal engine for Deep Down, which was never released; it became the RE Engine. |
| Pie in the Sky | no | No public documentation of this engine's game files was found. |
| Q | no | No public documentation of this engine's game files was found. |
| Shark 3D | no | No public documentation of this engine's game files was found. |
| Silent Storm engine | no | No public documentation of the archive format of Silent Storm's data was found. |
| Treyarch NGL | no | Treyarch's internal engine; no public documentation of its game files was found. |
| V-Play Game Engine (Felgo) | no | Felgo (V-Play) apps are Qt apps: their files are Qt's, not Felgo's. |
| Visual3D Game Engine | no | No public documentation of this engine's game files was found. |
