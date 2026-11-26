# Building the FNA kit

FNA, its native libraries and MonoKickstart have no prebuilt official releases, so the
kit is built once and reused for every XNA game (`--set runtime=PATH/TO/fna-kit`, which is
remembered). The kit is one folder:

```
fna-kit/
  FNA.dll  FNA.dll.config
  Microsoft.Xna.Framework.dll  Microsoft.Xna.Framework.Game.dll  Microsoft.Xna.Framework.Graphics.dll  ...
  mscorlib.dll  System.dll  System.Core.dll  ...        (Mono class library)
  monoconfig  monomachineconfig
  kick.bin.osx      osx/     (macOS: libmono, SDL, FNA3D, FAudio, Theorafile)
  kick.bin.x86_64   lib64/   (Linux: the same libraries)
```

1. **FNA and its XNA assemblies.** Download the latest FNA release
   (https://github.com/FNA-XNA/FNA/releases, for example `FNA-2611.zip`) and build it with the
   .NET SDK: `dotnet build -c Release FNA/FNA.NetFramework.csproj`, then each project in
   `FNA/abi/` (`Microsoft.Xna.Framework*.csproj`). Copy `FNA.dll`, `FNA.dll.config` and the
   `Microsoft.Xna.Framework*.dll` files into the kit. The abi assemblies forward XNA's types
   to FNA, so the game's own files are not changed.
2. **fnalibs.** From https://github.com/FNA-XNA/fnalibs-dailies, open the latest successful
   Actions run and download the `fnalibs` artifact (a GitHub sign-in is needed); copy its macOS
   libraries into `osx/` and its Linux x64 libraries into `lib64/`. FNA's documentation:
   https://fna-xna.github.io/docs/1%3A-Setting-Up-FNA/
3. **MonoKickstart.** Build it from https://github.com/flibitijibibo/MonoKickstart (CMake, see
   its README) for macOS and Linux, giving `kick.bin.osx` / `kick.bin.x86_64`, libmono for
   `osx/` / `lib64/`, the Mono class library (`mscorlib.dll`, `System*.dll`, `Mono.*.dll`) and
   `monoconfig` / `monomachineconfig`. If you own an FNA game with a Mac or Linux version, its
   folder has exactly these files; copying them from there works too.
4. On macOS, run MonoKickstart's `fixDylibs.sh` in the kit after adding libraries.

Porting Toolkit checks the kit before every export and names whatever is missing.
