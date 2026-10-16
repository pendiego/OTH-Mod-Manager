# over the hill — modding recon

- **Game**: Steam appid 2929250 (Funselektor), build `26.10.14_0a7187a`, Steam buildid `25931885`.
- **Install**: `D:\SteamLibrary\steamapps\common\over the hill`
- **Engine**: Unity 6000.0.75f1, IL2CPP (`GameAssembly.dll`, `global-metadata.dat` v31), Addressables, FMOD.
- **Anti-cheat**: none found. Online co-op uses Photon Fusion (P2P) + EOS. No loader pre-installed.
- **User data**: `%USERPROFILE%\Documents\My Games\over the hill\` — `Saves\Player1.json` (Newtonsoft JSON), `Settings\settings` (Odin binary), `LocalLiveries\`.
- **Log**: `%USERPROFILE%\AppData\LocalLow\Funselektor\over the hill\Player.log`

## Community route
- MelonLoader 0.7.3+ (13 of 17 Nexus mods; installs to `Mods/`, `UserData/`; needs .NET 6, MelonLoader installs it on first launch).
- BepInEx 6 IL2CPP be.788 (3 mods; `BepInEx/plugins/`). Only one loader active at a time (proxy DLLs: `version.dll` vs `winhttp.dll`).
- Official: livery packs (Funselektor Steam FAQ #24).

## Livery manifest
A pack is a subfolder of `LocalLiveries/` with `manifest.json`:
```json
{"liveries":[{"car":"…","name":"…","texture":"x.png","icon":"icon.png"}]}
```
`texture`/`icon` are relative to the manifest (types `ModLiveryService`, `ModLiveryManifest`, `ModLiveryEntry` in il2cpp metadata).

## Open unknowns
- Valid `car` values.
- Expected texture resolution.
- Loader "generated assemblies" marker paths (`MelonLoader/Il2CppAssemblies/Assembly-CSharp.dll`, `BepInEx/interop/Assembly-CSharp.dll`) unverified until a real launch.
