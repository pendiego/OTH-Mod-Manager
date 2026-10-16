<script lang="ts">
  import "@fontsource-variable/nunito-sans";
  import { onMount, tick } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath, openUrl } from "@tauri-apps/plugin-opener";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onOpenUrl, getCurrent } from "@tauri-apps/plugin-deep-link";
  import { api, type Account, type GameInfo, type InstallResult, type LiveryPack, type Loader, type LoaderStatus, type ModEntry, type NexusMod } from "$lib/api";
  import { i18n, setLang, t, tr } from "$lib/i18n.svelte";

  type Tab = "library" | "explore" | "settings";
  let tab = $state<Tab>("library");
  let game = $state<GameInfo | null>(null);
  let mods = $state<ModEntry[]>([]);
  let liveries = $state<LiveryPack[]>([]);
  let loaders = $state<LoaderStatus[]>([]);
  let nexus = $state<NexusMod[] | null>(null);
  let nexusErr = $state("");
  let toast = $state<{ msg: string; kind: "ok" | "err" | "info" } | null>(null);
  let busy = $state<Loader | null>(null);
  let progress = $state<{ received: number; total: number | null } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout>;
  let art = $state<string | null>(null);
  let dragging = $state(false);
  let q = $state("");
  let lf = $state<"all" | Loader>("all");
  let welcome = $state(localStorage.getItem("onboarded") !== "1");
  let wStep = $state(0);
  let dialog = $state<{ text: string; ok: string; danger: boolean; resolve: (v: boolean) => void } | null>(null);

  const LOADER_NAME: Record<Loader, string> = { melon: "MelonLoader", bepinex: "BepInEx" };

  function say(msg: string, kind: "ok" | "err" | "info" = "info") {
    toast = { msg, kind };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 5000);
  }
  async function run<T>(f: () => Promise<T>): Promise<T | undefined> {
    try {
      return await f();
    } catch (e) {
      say(tr(e), "err");
    }
  }
  function ask(text: string, ok: string, danger = false): Promise<boolean> {
    return new Promise((resolve) => {
      dialog = {
        text,
        ok,
        danger,
        resolve: (v) => {
          dialog = null;
          resolve(v);
        },
      };
    });
  }

  async function refresh() {
    const [m, l, s] = await Promise.all([
      api.listMods().catch(() => [] as ModEntry[]),
      api.listLiveries().catch(() => [] as LiveryPack[]),
      api.loadersStatus().catch(() => [] as LoaderStatus[]),
    ]);
    mods = m;
    liveries = l;
    loaders = s;
  }

  const status = (l: Loader) => loaders.find((s) => s.loader === l);
  function stateText(s: LoaderStatus) {
    if (!s.installed) return t("st_none");
    if (!s.active) return t("st_inactive");
    return s.generated ? t("st_active") : t("st_generate");
  }

  async function installLoader(l: Loader) {
    busy = l;
    progress = null;
    await run(() => api.installLoader(l));
    busy = null;
    progress = null;
    await refresh();
  }
  async function uninstallLoader(l: Loader) {
    if (await ask(t("confirmRemove", { name: LOADER_NAME[l] }), t("uninstall"), true)) {
      await run(() => api.uninstallLoader(l));
      await refresh();
    }
  }
  async function ensureLoader(l: Loader) {
    const s = status(l);
    if (s?.active) return;
    const o: Loader = l === "melon" ? "bepinex" : "melon";
    const other = status(o)?.active ? LOADER_NAME[o] : null;
    const loader = LOADER_NAME[l];
    const [text, ok] = !s?.installed
      ? [t("askInstallLoader", { loader }), t("install")]
      : other
        ? [t("askSwitchLoader", { loader, other }), t("activate")]
        : [t("askActivateLoader", { loader }), t("activate")];
    if (!(await ask(text, ok))) return;
    if (!s?.installed) await installLoader(l);
    else {
      await run(() => api.setActiveLoader(l));
      await refresh();
    }
    if (status(l)?.active) say(t("loaderReady", { loader }), "ok");
  }

  async function afterInstall(r: InstallResult | undefined) {
    if (!r) return;
    say(t("installed", { names: r.names.join(", ") }), "ok");
    await refresh();
    if (r.loader) await ensureLoader(r.loader);
  }
  async function installPaths(paths: string[]) {
    let loader: Loader | null = null;
    for (const p of paths) {
      const r = await run(() => api.installArchive(p));
      if (!r) continue;
      say(t("installed", { names: r.names.join(", ") }), "ok");
      loader ??= r.loader;
    }
    await refresh();
    if (loader) await ensureLoader(loader);
  }
  async function pick() {
    const r = await open({ multiple: true, filters: [{ name: "Mods", extensions: ["zip", "7z", "rar"] }] });
    if (r) await installPaths(Array.isArray(r) ? r : [r]);
  }
  async function changeDir() {
    const d = await open({ directory: true });
    if (typeof d === "string") {
      const g = await run(() => api.setGameDir(d));
      if (g) {
        game = g;
        await refresh();
      }
    }
  }
  function play() {
    if (loaders.some((l) => l.active && !l.generated)) say(t("firstLaunchSlow"));
    openUrl("steam://rungameid/2929250");
  }

  async function loadNexus() {
    nexusErr = "";
    try {
      nexus = await api.nexusMods();
    } catch (e) {
      nexusErr = tr(e);
    }
  }
  function goto(x: Tab) {
    tab = x;
    if (x === "explore" && !nexus) loadNexus();
  }

  let account = $state<Account | null>(null);
  let keyInput = $state("");
  let nxBusy = $state<number | null>(null);

  async function connect() {
    const a = await run(() => api.nexusConnect(keyInput));
    if (a) {
      account = a;
      keyInput = "";
    }
  }
  async function disconnect() {
    await run(api.nexusDisconnect);
    account = null;
  }
  async function nexusInstall(modId: number, fileId?: number, key?: string, expires?: number) {
    nxBusy = modId;
    progress = null;
    const r = await run(() => api.nexusInstall(modId, fileId, key, expires));
    nxBusy = null;
    progress = null;
    await afterInstall(r);
  }
  // dependências (exceto loaders, que o app instala): instalada = mesmo nexusId, ou nome idêntico (normalizado)
  // em mod sem nexusId (instalado à mão). Sem substring: poderia aceitar outro mod.
  const nameKey = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, "");
  const missingDeps = (m: NexusMod) =>
    m.requirements.filter(
      (r) =>
        !reqLoader(r.name) &&
        !mods.some((x) => (r.modId && x.nexusId === r.modId) || (x.nexusId === null && nameKey(x.name) === nameKey(r.name))),
    );
  async function installFromNexus(m: NexusMod) {
    const miss = missingDeps(m);
    if (miss.length) {
      if (await ask(t("depMissing", { names: miss.map((r) => r.name).join(", ") }), t("depDownload")))
        for (const r of miss) {
          const u = r.modId ? "https://www.nexusmods.com/overthehill/mods/" + r.modId : r.url;
          if (u) openUrl(u);
        }
      return;
    }
    if (!account) {
      say(t("nxNeedAccount"));
      return goto("settings");
    }
    if (account.premium) return nexusInstall(m.modId);
    say(t("nxFreeHint"));
    openUrl(`https://www.nexusmods.com/overthehill/mods/${m.modId}?tab=files`);
  }
  // nxm://overthehill/mods/<id>/files/<fid>?key=..&expires=..  (botão "Mod Manager Download" do site)
  function onNxm(urls: string[] | null) {
    for (const raw of urls ?? []) {
      let u: URL;
      try {
        u = new URL(raw);
      } catch {
        continue;
      }
      const [, , mod, , file] = u.pathname.split("/");
      if (u.protocol !== "nxm:" || u.host !== "overthehill" || !Number(mod)) continue;
      tab = "explore";
      nexusInstall(Number(mod), Number(file) || undefined, u.searchParams.get("key") ?? undefined, Number(u.searchParams.get("expires")) || undefined);
    }
  }
  const reqLoader = (n: string): Loader | null => (/melonloader/i.test(n) ? "melon" : /bepinex/i.test(n) ? "bepinex" : null);
  const neededLoader = (m: NexusMod): Loader | null => m.loader ?? m.requirements.map((r) => reqLoader(r.name)).find(Boolean) ?? null;

  const shown = $derived(
    (nexus ?? []).filter(
      (m) =>
        (lf === "all" || m.loader === lf || m.requirements.some((r) => reqLoader(r.name) === lf)) &&
        `${m.name} ${m.author} ${m.summary}`.toLowerCase().includes(q.trim().toLowerCase()),
    ),
  );

  const norm = (v: string | null) => (v ?? "").trim().replace(/^v/i, "").replace(/[.\s]+/g, "-").toLowerCase();
  const installedOf = (m: NexusMod) => mods.find((x) => x.nexusId === m.modId);
  // ponytail: compara a versão da página do mod com a do arquivo; pode divergir se o autor não atualizar o campo da página
  const updateOf = (m: NexusMod) => {
    const inst = installedOf(m);
    return !!inst && !!inst.version && norm(inst.version) !== norm(m.version);
  };
  const mb = (n: number) => (n / 1048576).toFixed(1);

  // tour guiado: roda uma vez, logo depois das boas-vindas
  const STEPS = [
    { tab: "library", sel: "nav .tab:nth-child(1)", key: "tourLibrary" },
    { tab: "library", sel: "main .bar .primary", key: "tourInstall" },
    { tab: "explore", sel: "nav .tab:nth-child(2)", key: "tourExplore" },
    { tab: "settings", sel: "nav .tab:nth-child(3)", key: "tourSettings" },
    { tab: "library", sel: "header .primary", key: "tourPlay" },
  ] as const;
  let tour = $state<number | null>(null);
  let rect = $state<DOMRect | null>(null);
  function endTour() {
    tour = null;
    localStorage.setItem("toured", "1");
    goto("explore");
  }
  $effect(() => {
    if (tour === null) return;
    const s = STEPS[tour];
    tab = s.tab;
    tick().then(() => (rect = document.querySelector(s.sel)?.getBoundingClientRect() ?? null));
  });

  function finishWelcome() {
    localStorage.setItem("onboarded", "1");
    welcome = false;
    if (localStorage.getItem("toured") === "1") goto("explore");
    else tour = 0;
  }

  onMount(() => {
    (async () => {
      game = (await run(api.getGame)) ?? null;
      await refresh();
    })();
    api.nexusAccount().then((a) => (account = a)).catch(() => {});
    api.gameArt().then((a) => (art = a)).catch(() => {});
    loadNexus();
    getCurrent().then(onNxm).catch(() => {});
    const un = [
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === "enter" || p.type === "over") dragging = true;
        else if (p.type === "leave") dragging = false;
        else if (p.type === "drop") {
          dragging = false;
          const ok = p.paths.filter((x) => /\.(zip|7z|rar)$/i.test(x));
          if (ok.length < p.paths.length) say(tr("Formato não suportado"), "err");
          if (ok.length) installPaths(ok);
        }
      }),
      listen<{ received: number; total: number | null }>("download-progress", (e) => (progress = e.payload)),
      onOpenUrl(onNxm),
    ];
    return () => un.forEach((u) => u.then((f) => f()));
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && dialog?.resolve(false)} />

<div class="scene" class:art style:background-image={art ? `url(${art})` : null}></div>

<header>
  <div class="brand"><span class="logo">over the hill</span><span class="sub">{t("appSub")}</span></div>
  <nav>
    {#each [["library", "tabLibrary"], ["explore", "tabExplore"], ["settings", "tabSettings"]] as const as [id, k]}
      <button class="tab" class:on={tab === id} onclick={() => goto(id)}>{t(k)}</button>
    {/each}
  </nav>
  <button class="primary big" onclick={play}>{t("play")}</button>
</header>

{#if !game?.dir}
  <div class="banner">
    <span>{t("gameMissing")}</span>
    <button class="ghost" onclick={changeDir}>{t("chooseFolder")}</button>
  </div>
{/if}

<main>
  {#if tab === "library"}
    <div class="bar">
      <button class="primary" onclick={pick}>{t("installFile")}</button>
      <span class="muted">{t("dropHint")}</span>
    </div>

    {#if mods.length === 0 && liveries.length === 0}
      <div class="panel empty">
        <h2>{t("emptyTitle")}</h2>
        <p class="muted">{t("emptyBody")}</p>
        <div class="bar center">
          <button class="primary" onclick={() => goto("explore")}>{t("exploreCta")}</button>
          <button class="ghost" onclick={pick}>{t("installFile")}</button>
        </div>
      </div>
    {:else}
      {#if mods.length}
        <h2>{t("secMods")} · {mods.length}</h2>
        <div class="stack">
          {#each mods as m (m.id)}
            <div class="row">
              <b>{m.name}</b>
              {#if m.version}<span class="muted">{m.version}</span>{/if}
              <span class="key">{LOADER_NAME[m.loader]}</span>
              {#if !m.managed}<span class="key" title={t("manualTip")}>{t("manual")}</span>{/if}
              {#if !status(m.loader)?.active}
                <span class="chip warn">{t("loaderOff", { loader: LOADER_NAME[m.loader] })} <button class="link" onclick={() => ensureLoader(m.loader)}>{t("fix")}</button></span>
              {/if}
              <span class="grow"></span>
              <input
                type="checkbox"
                role="switch"
                class="switch"
                checked={m.enabled}
                onchange={async (e) => {
                  await run(() => api.setModEnabled(m.id, e.currentTarget.checked));
                  await refresh();
                }}
              />
              <button
                class="ghost"
                onclick={async () => {
                  if (await ask(t("confirmRemove", { name: m.name }), t("remove"), true)) {
                    await run(() => api.uninstallMod(m.id));
                    await refresh();
                  }
                }}>{t("remove")}</button
              >
            </div>
          {/each}
        </div>
      {/if}

      {#if liveries.length}
        <div class="bar between">
          <h2>{t("secLiveries")} · {liveries.length}</h2>
          <button class="ghost" onclick={() => game && openPath(game.liveriesDir)}>{t("openFolder")}</button>
        </div>
        <div class="grid">
          {#each liveries as p (p.folder)}
            <div class="card">
              {#if p.thumb}<img src={p.thumb} alt={p.folder} />{:else}<div class="ph"></div>{/if}
              <b>{p.folder}</b>
              {#each p.entries as e}<div class="muted">{e.car} — {e.name}</div>{/each}
              {#if p.error}<div class="err">{tr(p.error)}</div>{/if}
              <div class="actions">
                <input
                  type="checkbox"
                  role="switch"
                  class="switch"
                  checked={p.enabled}
                  onchange={async (e) => {
                    await run(() => api.setLiveryEnabled(p.folder, e.currentTarget.checked));
                    await refresh();
                  }}
                />
                <button
                  class="ghost"
                  onclick={async () => {
                    if (await ask(t("confirmRemove", { name: p.folder }), t("remove"), true)) {
                      await run(() => api.removeLivery(p.folder));
                      await refresh();
                    }
                  }}>{t("remove")}</button
                >
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  {:else if tab === "explore"}
    {#if !account}
      <div class="panel bar between">
        <span>{t("nxBanner")}</span>
        <button class="primary" onclick={() => goto("settings")}>{t("nxGoSettings")}</button>
      </div>
    {/if}
    <div class="bar">
      <input type="search" bind:value={q} placeholder={t("search")} />
      <div class="seg">
        <button class:on={lf === "all"} onclick={() => (lf = "all")}>{t("fAll")}</button>
        <button class:on={lf === "melon"} onclick={() => (lf = "melon")}>MelonLoader</button>
        <button class:on={lf === "bepinex"} onclick={() => (lf = "bepinex")}>BepInEx</button>
      </div>
    </div>
    {#if nexusErr}
      <div class="bar"><span class="err">{nexusErr}</span><button class="ghost" onclick={loadNexus}>{t("retry")}</button></div>
    {:else if !nexus}
      <p class="muted">{t("loading")}</p>
    {:else if shown.length === 0}
      <p class="muted">{t("noResults")}</p>
    {:else}
      <div class="grid">
        {#each shown as m (m.modId)}
          {@const inst = installedOf(m)}
          {@const upd = updateOf(m)}
          {@const need = neededLoader(m)}
          <div class="card">
            <div class="thumb">
              {#if m.thumbnailUrl}<img src={m.thumbnailUrl} alt={m.name} />{:else}<div class="ph"></div>{/if}
              {#if upd}<span class="badge badge-up">{t("badgeUpdate")}</span>{:else if inst}<span class="badge badge-ok">{t("badgeInstalled")}</span>{/if}
            </div>
            <b>{m.name}</b>
            <div class="muted">{m.author} · v{m.version}</div>
            <div class="summary">{m.summary}</div>
            {#if m.requirements.length}
              <div class="req">
                {t("nxRequires")}:
                {#each m.requirements as r, i}
                  {#if i}, {/if}
                  {#if r.modId}<button class="link" title={r.notes} onclick={() => openUrl("https://www.nexusmods.com/overthehill/mods/" + r.modId)}>{r.name}</button>{:else if r.url}<button class="link" title={r.notes} onclick={() => openUrl(r.url)}>{r.name}</button>{:else}<span title={r.notes}>{r.name}</span>{/if}
                {/each}
              </div>
            {/if}
            {#if need && !status(need)?.active}<div class="muted">{t("needsLoader", { loader: LOADER_NAME[need] })}</div>{/if}
            <div class="actions">
              <button class={inst && !upd ? "ghost" : "primary"} disabled={nxBusy !== null} onclick={() => installFromNexus(m)}>
                {nxBusy === m.modId ? t("nxInstalling") : upd ? t("nxUpdate") : inst ? t("nxReinstall") : t("nxInstall")}
              </button>
              <button class="ghost" onclick={() => openUrl("https://www.nexusmods.com/overthehill/mods/" + m.modId)}>{t("openOnNexus")}</button>
              <span class="muted stats">↓ {m.downloads} · ♥ {m.endorsements}</span>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="settings">
      <section class="panel">
        <h2>{t("secGame")}</h2>
        <div class="path">{game?.dir ?? t("gameMissing")}</div>
        <div class="bar">
          <button class="ghost" onclick={changeDir}>{t("change")}</button>
          <button class="ghost" disabled={!game?.dir} onclick={() => game?.dir && openPath(game.dir)}>{t("openFolder")}</button>
        </div>
      </section>

      <section class="panel">
        <h2>{t("secNexus")}</h2>
        {#if account}
          <div class="bar">
            <span>{t("nxAs", { name: account.name })} · {account.premium ? t("nxPremium") : t("nxFree")}</span>
            <button class="ghost" onclick={disconnect}>{t("nxDisconnect")}</button>
          </div>
        {:else}
          <div class="bar">
            <input type="password" bind:value={keyInput} placeholder={t("nxKeyPh")} />
            <button class="primary" disabled={!keyInput.trim()} onclick={connect}>{t("nxConnect")}</button>
            <button class="ghost" onclick={() => openUrl("https://www.nexusmods.com/settings/api-keys")}>{t("nxGetKey")}</button>
          </div>
        {/if}
        <span class="muted">{t("nxKeyHelp")}</span>
      </section>

      <section class="panel">
        <h2>{t("secLang")}</h2>
        <div class="seg">
          <button class:on={i18n.lang === "pt"} onclick={() => setLang("pt")}>Português</button>
          <button class:on={i18n.lang === "en"} onclick={() => setLang("en")}>English</button>
        </div>
      </section>

      <section class="panel">
        <h2>{t("secAdvanced")}</h2>
        <span class="muted">{t("advHint")}</span>
        <div class="stack">
          {#each loaders as s (s.loader)}
            <div class="row col">
              <b>{s.label}</b>
              <div class:ok={s.active}>{stateText(s)}</div>
              {#if s.loader === "melon"}<div class="muted">{t("note_melon")}</div>{/if}
              <div class="muted">{t("note_common")}</div>
              <div class="actions">
                {#if !s.installed}
                  <button class="primary" disabled={busy !== null || !game?.dir} onclick={() => installLoader(s.loader)}>{t("install")}</button>
                {:else}
                  {#if s.active}
                    <button
                      class="ghost"
                      onclick={async () => {
                        await run(() => api.setActiveLoader(null));
                        await refresh();
                      }}>{t("deactivate")}</button
                    >
                  {:else}
                    <button
                      class="primary"
                      onclick={async () => {
                        await run(() => api.setActiveLoader(s.loader));
                        await refresh();
                      }}>{t("activate")}</button
                    >
                  {/if}
                  <button class="ghost" onclick={() => uninstallLoader(s.loader)}>{t("uninstall")}</button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </section>
    </div>
  {/if}
</main>

{#if progress !== null}
  <div class="dl panel">
    <span>{progress.total === null ? t("downloadingNoTotal", { got: mb(progress.received) }) : t("downloading", { got: mb(progress.received), total: mb(progress.total) })}</span>
    <progress max={progress.total ?? undefined} value={progress.total === null ? undefined : progress.received}></progress>
  </div>
{/if}

{#if welcome}
  <div class="welcome">
    <div class="panel wpanel">
      {#if wStep === 0}
        <h2>{t("wTitle")}</h2>
        <div class="seg">
          <button class:on={i18n.lang === "pt"} onclick={() => setLang("pt")}>Português</button>
          <button class:on={i18n.lang === "en"} onclick={() => setLang("en")}>English</button>
        </div>
        {#if game?.dir}
          <div>{t("wGameFound")}</div>
          <div class="path">{game.dir}</div>
        {:else}
          <div>{t("wGameMissing")}</div>
          <div><button class="ghost" onclick={changeDir}>{t("wBrowse")}</button></div>
        {/if}
        <div class="bar between">
          <button class="link" onclick={() => (wStep = 1)}>{t("skip")}</button>
          <button class="primary" disabled={!game?.dir} onclick={() => (wStep = 1)}>{t("next")}</button>
        </div>
      {:else if wStep === 1}
        <h2>{t("wNexusTitle")}</h2>
        <div class="muted">{t("wNexusBody")}</div>
        {#if account}
          <div>{t("nxAs", { name: account.name })}</div>
        {:else}
          <div class="bar">
            <input type="password" bind:value={keyInput} placeholder={t("nxKeyPh")} />
            <button class="primary" disabled={!keyInput.trim()} onclick={connect}>{t("nxConnect")}</button>
            <button class="ghost" onclick={() => openUrl("https://www.nexusmods.com/settings/api-keys")}>{t("nxGetKey")}</button>
          </div>
        {/if}
        <div class="bar between">
          <button class="link" onclick={() => (wStep = 2)}>{t("skip")}</button>
          <button class="primary" onclick={() => (wStep = 2)}>{t("next")}</button>
        </div>
      {:else}
        <h2>{t("wDoneTitle")}</h2>
        <ul>
          <li>{t("wDone1")}</li>
          <li>{t("wDone2")}</li>
          <li>{t("wDone3")}</li>
        </ul>
        <div class="bar between">
          <span></span>
          <button class="primary" onclick={finishWelcome}>{t("start")}</button>
        </div>
      {/if}
      <div class="dots">{#each [0, 1, 2] as i}<span class:on={wStep === i}></span>{/each}</div>
    </div>
  </div>
{/if}

{#if tour !== null}
  {@const s = STEPS[tour]}
  <div class="blocker"></div>
  {#if rect}<div class="spot" style:top="{rect.top - 4}px" style:left="{rect.left - 4}px" style:width="{rect.width + 8}px" style:height="{rect.height + 8}px"></div>{/if}
  <div class="tip panel" style:top="{(rect?.bottom ?? 80) + 14}px" style:left="{Math.min(Math.max(8, rect?.left ?? 8), innerWidth - 328)}px">
    <p>{t(s.key)}</p>
    <div class="bar between">
      <span class="muted">{tour + 1} / {STEPS.length}</span>
      <span class="bar">
        <button class="link" onclick={endTour}>{t("skip")}</button>
        <button class="primary" onclick={() => (tour! + 1 < STEPS.length ? tour!++ : endTour())}>{tour + 1 < STEPS.length ? t("next") : t("tourDone")}</button>
      </span>
    </div>
  </div>
{/if}

{#if dragging}
  <div class="drop">
    <div class="dropbox"><div class="droptitle">{t("dropHere")}</div><div>{t("dropSub")}</div></div>
  </div>
{/if}

{#if dialog}
  {@const d = dialog}
  <div class="backdrop" role="presentation" onclick={() => d.resolve(false)}>
    <div class="dialog panel" role="alertdialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
      <p>{d.text}</p>
      <div class="bar end">
        <button class="ghost" onclick={() => d.resolve(false)}>{t("cancel")}</button>
        <button class={d.danger ? "danger" : "primary"} onclick={() => d.resolve(true)}>{d.ok}</button>
      </div>
    </div>
  </div>
{/if}

{#if toast}<div class="toast panel {toast.kind}">{toast.msg}</div>{/if}

<style>
  :global(:root) {
    color-scheme: dark;
    --fg: #ffffff; --fg-soft: #ece8df; --muted: #bdb8ad;
    --panel: rgb(34 36 42 / 0.72); --row: rgb(58 60 68 / 0.62); --row-hover: rgb(78 80 90 / 0.7);
    --border: rgb(255 255 255 / 0.08);
    --accent: #f7c64a; --on-accent: #26262b;
    --ok: #8fd18b; --ok-bg: rgb(143 209 139 / 0.9);
    --danger: #ff7a66; --danger-bg: rgb(220 70 50 / 0.9);
    --key: #f4f2ee; --on-key: #26262b;
    --focus: #f7c64a; --scroll: rgb(255 255 255 / 0.25);
  }
  :global(:focus-visible) { outline: 2px solid var(--focus); outline-offset: 2px; }
  :global(::-webkit-scrollbar) { width: 10px; height: 10px; }
  :global(::-webkit-scrollbar-thumb) { background: var(--scroll); border-radius: 5px; border: 3px solid transparent; background-clip: padding-box; }
  :global(::-webkit-scrollbar-button) { display: none; }
  :global(html) { background: #1b1d22; }
  :global(body) { margin: 0; font: 15px/1.45 "Nunito Sans Variable", "Segoe UI", system-ui, sans-serif; color: var(--fg); height: 100vh; display: flex; flex-direction: column; overflow: hidden; }
  :global(#svelte) { height: 100%; display: flex; flex-direction: column; }

  .scene { position: fixed; inset: 0; z-index: -1; background: linear-gradient(180deg, #5cc3e4 0%, #a6dbe9 45%, #e9b95c 72%, #d4973a 100%) center / cover; }
  .scene.art { background-size: cover; background-position: center 70%; }
  .scene::after { content: ""; position: absolute; inset: 0; background: linear-gradient(180deg, rgb(18 20 26 / 0.55), rgb(18 20 26 / 0.82)); }

  header { display: flex; gap: 16px; align-items: center; justify-content: space-between; padding: 14px 24px; }
  .brand { display: flex; flex-direction: column; line-height: 1.1; }
  .logo { font-weight: 900; font-size: 28px; letter-spacing: -0.02em; text-transform: lowercase; text-shadow: 0 2px 12px rgb(0 0 0 / 0.35); }
  .sub { text-transform: uppercase; font-size: 11px; letter-spacing: 0.24em; color: var(--muted); }
  nav { display: flex; gap: 2px; }
  .tab, h2 { text-transform: uppercase; font-weight: 700; letter-spacing: 0.06em; font-size: 14px; }
  .tab { background: var(--row); color: var(--fg); padding: 8px 18px; border-radius: 4px; }
  .tab:hover:not(.on) { background: var(--row-hover); }
  .tab.on { background: var(--accent); color: var(--on-accent); }
  h2 { margin: 8px 0 0; }

  .banner { display: flex; gap: 12px; align-items: center; justify-content: center; padding: 8px 24px; background: var(--danger-bg); }
  main { flex: 1; overflow-y: auto; padding: 24px; max-width: 1200px; margin: 0 auto; width: 100%; box-sizing: border-box; display: flex; flex-direction: column; gap: 12px; }

  button { font: inherit; border: 0; border-radius: 4px; padding: 7px 16px; cursor: pointer; color: inherit; }
  button:disabled { opacity: 0.45; cursor: default; }
  .primary { background: var(--accent); color: var(--on-accent); font-weight: 800; text-transform: uppercase; }
  .primary:hover:not(:disabled) { filter: brightness(1.08); }
  .primary.big { padding: 10px 28px; font-size: 16px; }
  .ghost { background: var(--row); font-weight: 700; text-transform: uppercase; font-size: 13px; }
  .ghost:hover:not(:disabled) { background: var(--row-hover); }
  .danger { background: var(--danger-bg); color: #fff; font-weight: 800; text-transform: uppercase; }
  .link { background: none; padding: 0; color: var(--accent); text-decoration: underline; }

  .panel { background: var(--panel); backdrop-filter: blur(14px); border: 1px solid var(--border); border-radius: 8px; padding: 14px 16px; }
  .bar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .bar.between { justify-content: space-between; }
  .bar.center { justify-content: center; }
  .bar.end { justify-content: flex-end; }
  .stack { display: flex; flex-direction: column; gap: 2px; }
  .row { display: flex; gap: 8px; align-items: center; background: var(--row); backdrop-filter: blur(14px); border: 1px solid var(--border); border-radius: 4px; padding: 8px 14px; }
  .row:hover { background: var(--row-hover); }
  .row.col { flex-direction: column; align-items: flex-start; }
  .grow { flex: 1; }
  .end { text-align: right; }
  .empty { text-align: center; padding: 48px 16px; display: flex; flex-direction: column; gap: 8px; align-items: center; }

  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 12px; }
  .card { display: flex; flex-direction: column; gap: 6px; background: var(--row); backdrop-filter: blur(14px); border: 1px solid var(--border); border-radius: 8px; padding: 10px 14px 14px; }
  .card:hover { background: var(--row-hover); }
  .thumb { position: relative; }
  .card img, .ph { width: 100%; aspect-ratio: 16/9; object-fit: cover; border-radius: 4px; background: rgb(255 255 255 / 0.08); display: block; }
  .summary { display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; color: var(--fg-soft); }
  .req { font-size: 13px; color: var(--fg-soft); }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; margin-top: auto; padding-top: 6px; }
  .actions .stats { flex: 1 0 100%; font-size: 13px; }
  .actions .ghost, .actions .primary { white-space: nowrap; }
  .actions:has(.switch) { justify-content: space-between; }

  .key { background: var(--key); color: var(--on-key); font-weight: 800; font-size: 11px; padding: 1px 6px; border-radius: 3px; }
  .badge { position: absolute; top: 6px; left: 6px; text-transform: uppercase; font-size: 11px; font-weight: 800; padding: 2px 8px; border-radius: 3px; }
  .badge-ok { background: var(--ok-bg); color: var(--on-accent); }
  .badge-up { background: var(--accent); color: var(--on-accent); }
  .chip.warn { border: 1px solid var(--accent); color: var(--accent); border-radius: 4px; padding: 1px 8px; font-size: 13px; }
  .muted { color: var(--muted); }
  .err { color: var(--danger); }
  .ok { color: var(--ok); }

  .switch { appearance: none; position: relative; width: 36px; height: 20px; border-radius: 10px; background: var(--row); border: 1px solid var(--border); margin: 0; cursor: pointer; transition: background 0.15s; flex: none; }
  .switch::before { content: ""; position: absolute; top: 1px; left: 1px; width: 16px; height: 16px; border-radius: 50%; background: #fff; transition: transform 0.15s; }
  .switch:checked { background: var(--accent); }
  .switch:checked::before { transform: translateX(16px); }

  input[type="search"], input[type="password"] { background: var(--row); border: 1px solid var(--border); border-radius: 4px; padding: 7px 10px; color: var(--fg); font: inherit; flex: 1; min-width: 160px; max-width: 420px; }
  input:focus-visible { outline: 2px solid var(--accent); outline-offset: 0; }
  .seg { display: flex; gap: 2px; }
  .seg button { background: var(--row); font-weight: 700; }
  .seg button.on { background: var(--accent); color: var(--on-accent); }

  .settings { display: flex; flex-direction: column; gap: 12px; max-width: 720px; }
  .settings section { display: flex; flex-direction: column; gap: 8px; }
  .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--fg-soft); }

  .dl { position: fixed; right: 16px; bottom: 16px; width: 280px; display: flex; flex-direction: column; gap: 6px; z-index: 40; font-size: 13px; }
  progress { width: 100%; height: 8px; appearance: none; border: 0; border-radius: 4px; overflow: hidden; background: var(--row); }
  progress::-webkit-progress-bar { background: var(--row); }
  progress::-webkit-progress-value { background: var(--accent); }

  .welcome { position: fixed; inset: 0; z-index: 50; background: rgb(0 0 0 / 0.45); display: grid; place-items: center; }
  .wpanel { width: 520px; max-width: calc(100vw - 32px); display: flex; flex-direction: column; gap: 14px; padding: 24px; }
  .wpanel h2 { font-size: 18px; margin: 0; }
  .dots { display: flex; gap: 6px; justify-content: center; }
  .dots span { width: 8px; height: 8px; border-radius: 50%; background: var(--row-hover); }
  .dots span.on { background: var(--accent); }

  .backdrop { position: fixed; inset: 0; z-index: 60; background: rgb(0 0 0 / 0.5); display: grid; place-items: center; }
  .dialog { width: 420px; max-width: calc(100vw - 32px); display: flex; flex-direction: column; gap: 8px; }
  .dialog p { margin: 0 0 8px; }

  .toast { position: fixed; bottom: 16px; left: 50%; transform: translateX(-50%); max-width: 80vw; z-index: 70; border-left: 4px solid var(--accent); }
  .toast.ok { border-left-color: var(--ok); }
  .toast.err { border-left-color: var(--danger); }

  .blocker { position: fixed; inset: 0; z-index: 90; }
  .spot { position: fixed; z-index: 91; pointer-events: none; box-sizing: border-box; border: 2px solid var(--accent); border-radius: 6px; box-shadow: 0 0 0 9999px rgb(0 0 0 / 0.6); }
  .tip { position: fixed; z-index: 92; width: 320px; display: flex; flex-direction: column; gap: 8px; }
  .tip p { margin: 0; }

  .drop { position: fixed; inset: 0; z-index: 80; pointer-events: none; background: rgb(0 0 0 / 0.35); padding: 16px; box-sizing: border-box; }
  .dropbox { height: 100%; box-sizing: border-box; border: 3px dashed var(--accent); border-radius: 8px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; }
  .droptitle { font-size: 40px; font-weight: 900; text-transform: uppercase; color: var(--accent); }

  @media (prefers-reduced-motion: reduce) {
    :global(*), .switch, .switch::before { transition: none !important; }
  }
</style>
