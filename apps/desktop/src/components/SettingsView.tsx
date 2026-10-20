import { useEffect, useState } from "react";
import {
  getSettingsStrict,
  getTrustedWorkspaces,
  readSearchEngines,
  saveSearchEngines,
  probeSearchEngine,
  type SearchEngine,
  type SearchEngineEdit,
  setCompactionSettings,
  setContextBar,
  setOnboarded,
  setPdfSettings,
  setProxySettings,
  setScratchBase,
  setSessionsPeek,
  setWorkspaceTrusted,
  type CompactionSettings,
  type ModelSettings,
  type PdfSettings,
  type ProxyMode,
  type ProxySettings,
  type WorkspaceCommandTrust,
} from "../api";
import {
  cancelDictationDownload,
  deleteDictationModel,
  downloadDictationModel,
  getAutostart,
  getDictationStatus,
  getKeepAwake,
  checkForUpdate,
  installUpdate,
  isTauri,
  listenDictationDone,
  listenDictationProgress,
  markDictationPassed,
  pickFolder,
  setAutostart,
  setKeepAwake,
  startDictation,
  stopDictation,
  verifyDictationModel,
  type DictationDownloadProgress,
  type DictationStatus,
} from "../tauri";
import { UPDATER_FEED_PUBLISHED } from "./UpdateBanner";
import { useThemePref } from "../theme";
import { LoadError } from "./ViewStates";
import { useI18n, type TranslationKey } from "@delta/i18n/I18nContext";
import type { Locale } from "@delta/i18n/types";
import { Icon } from "./Icon";
import { PanelHead } from "./IntegrationsView";
import { useImportantOpsPref } from "./ApprovalModal";
import { Toggle } from "./Toggle";
import { PrivacySection } from "./PrivacySection";
import { ModelsTab } from "./ManageTabs";
import { MemorySection } from "./MemorySection";
import { SkillsTab } from "./SkillsTab";

// Settings, restructured (Option 2) into a full-page surface that mirrors IntegrationsView's shell:
// a left sub-nav (Appearance · Files · Models · Skills) + centered panel, replacing the old
// top-tab ManageModal. Local/app concerns live here; anything external (Connectors, Messaging, MCP,
// Activity) stays under Integrations. Appearance + Files are re-skinned to the mock's Tailwind idiom;
// Models + Skills host the existing tab components inside the page shell.
// "appearance" is the General tab's stable key — callers deep-link with it, so the
// rename (UX-021) changed only the label. "files" folded into General as a card.
type SetTab = "appearance" | "models" | "privacy" | "skills" | "voice" | "memory";

const CARD = "rounded-xl2 border border-line bg-panel";
const ZH_CN_LABEL = "简体中文";
const FIELD_LABEL = "text-[12.5px] font-medium text-ink";
const FIELD_HELP = "text-[12px] text-muted mt-1.5 leading-relaxed";
const INPUT =
  "flex-1 min-w-0 px-3 py-2 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary";
const BTN_ACCENT = "text-[12.5px] px-3 py-2 rounded-lg bg-primary text-on-primary shrink-0 disabled:opacity-40";
const BTN_BORDERED =
  "text-[12.5px] px-3 py-2 rounded-lg border border-line bg-bg hover:border-line-strong shrink-0";

const SET_TABS: {
  key: SetTab;
  labelKey: TranslationKey;
  icon: "sliders" | "code" | "shield" | "mic" | "archive" | "book";
}[] = [
  { key: "appearance", labelKey: "settings.general.title", icon: "sliders" },
  { key: "models", labelKey: "settings.models.title", icon: "code" },
  { key: "privacy", labelKey: "settings.privacy.title", icon: "shield" },
  { key: "skills", labelKey: "settings.skills.title", icon: "book" },
  { key: "voice", labelKey: "settings.voice.title", icon: "mic" },
  { key: "memory", labelKey: "settings.memory.title", icon: "archive" },
];

export function SettingsView({
  initialTab,
  onCreateSkill,
}: {
  initialTab?: SetTab;
  // Skills doorway (SKILLS-SPEC §5.2): start a new conversation with the description
  // prefilled — the worker builds the skill and proposes it via save_skill.
  onCreateSkill?: (description: string) => void;
}) {
  const { t } = useI18n();
  const wanted = initialTab || "appearance";
  const [tab, setTab] = useState<SetTab>(wanted);

  // Deep-link sync: `initialTab` (settingsTab in App) can change while SettingsView is already
  // mounted (e.g. the composer's mic calls openSettings("voice") from inside another settings
  // tab). Without this, the inner tab would keep its old selection and the voice page would
  // appear to open and immediately vanish. Kept as an effect (not keying the component) so the
  // page never full-remounts — the window-flash fix.
  useEffect(() => {
    setTab(wanted);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wanted]);

  return (
    <main className="flex-1 min-w-0 flex bg-bg">
      <nav className="page-subnav w-[208px] shrink-0 border-r border-line bg-panel/40 px-3 py-4">
        <div className="px-2 text-[13.5px] font-semibold mb-3 flex items-center gap-2">
          <Icon name="gear" size={16} /> {t("settings.title")}
        </div>
        {SET_TABS.map((tb) => {
          const isActive = tab === tb.key;
          return (
            <button
              key={tb.key}
              className={
                "w-full text-left px-2.5 py-2 rounded-lg text-[13px] flex items-center gap-2 " +
                (isActive ? "bg-bg text-primary font-medium" : "text-muted hover:bg-bg hover:text-ink")
              }
              onClick={() => setTab(tb.key)}
            >
              <Icon name={tb.icon} size={15} /> {t(tb.labelKey)}
            </button>
          );
        })}
      </nav>

      <div className="flex-1 min-w-0 overflow-y-auto hairline-scroll">
        <div className="max-w-3xl mx-auto px-7 py-6">
          {tab === "appearance" ? (
            <>
              <AppearanceSection />
              {/* Network and web search are general behaviour of the app (every outbound call follows
                  the network route; search is a general capability), so they sit beside the other
                  general settings, not under Models or Privacy. */}
              <div className="mt-6">
                <NetworkProxyCard />
              </div>
              <div className="mt-6">
                <WebSearchCard />
              </div>
              <div className="mt-6">
                <ApprovalDialogCard />
              </div>
            </>
          ) : tab === "models" ? (
            <section>
              <PanelHead
                title={t("settings.models.title")}
                sub={t("settings.models.sub")}
              />
              <ModelsTab />
              {/* Token savings is model-spend behavior, so it lives here (UX-021),
                  not under General. */}
              <div className="mt-6">
                <TokenSavingsCard />
                <CompactionCard />
              </div>
            </section>
          ) : tab === "privacy" ? (
            <section>
              <PanelHead title={t("settings.privacy.title")} sub={t("settings.privacy.sub")} />
              <PrivacySection />
            </section>
          ) : tab === "skills" ? (
            <SkillsTab onCreateSkill={onCreateSkill} />
          ) : tab === "voice" ? (
            <VoiceInputSection />
          ) : tab === "memory" ? (
            <MemorySection />
          ) : null}
        </div>
      </div>
    </main>
  );
}

// -- Voice input: deliberate model provisioning + compatibility + microphone test (§37) --------
const voiceError = (error: unknown, t: (k: string, v?: Record<string, string | number>, f?: string) => string) =>
  error instanceof Error
    ? error.message
    : typeof error === "string"
      ? error
      : t("settings.voice.errorGeneric", undefined, "Voice Input could not complete that action.");

const formatBytes = (bytes: number) => {
  if (!bytes) return "0 MiB";
  return `${Math.round(bytes / 1024 / 1024)} MiB`;
};

function VoiceInputSection() {
  const { t } = useI18n();
  const [status, setStatus] = useState<DictationStatus | null>(null);
  const [progress, setProgress] = useState<DictationDownloadProgress | null>(null);
  const [phase, setPhase] = useState<"idle" | "verifying" | "testing" | "transcribing">("idle");
  const [error, setError] = useState<string | null>(null);
  const [testTranscript, setTestTranscript] = useState("");
  const desktop = isTauri();

  const publish = (next: DictationStatus) => {
    setStatus(next);
    window.dispatchEvent(new CustomEvent("delta:voice-input-changed", { detail: next }));
  };

  useEffect(() => {
    if (!desktop) return;
    let isActive = true;
    let unlisten = () => {};
    let unlistenFinished = () => {};
    void listenDictationProgress((next) => {
      if (isActive) setProgress(next);
    }).then((stop) => {
      unlisten = stop;
    });
    // The download runs in the background, so it can finish while this page is closed; the event
    // only adds the reason on a failure, the status read below is what the page shows.
    void listenDictationDone((error) => {
      if (!isActive) return;
      if (error) setError(error);
      void getDictationStatus().then((latest) => {
        if (isActive && latest) publish(latest);
      });
    }).then((stop) => {
      unlistenFinished = stop;
    });
    void getDictationStatus().then(async (initial) => {
      if (!isActive || !initial) return;
      publish(initial);
      // One-time migration for models installed by the first STT cut, before verification markers.
      if (initial.model_installed && !initial.model_verified) {
        setPhase("verifying");
        try {
          const verified = await verifyDictationModel();
          if (isActive) publish(verified);
        } catch (verifyError) {
          if (isActive) setError(voiceError(verifyError, t));
        } finally {
          if (isActive) setPhase("idle");
        }
      }
    });
    return () => {
      isActive = false;
      unlisten();
      unlistenFinished();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [desktop]);

  // While the download runs in the background, re-read its state so progress and the finish show
  // even if the events were missed (a page reopened mid-download starts from the status alone).
  const downloadingNow = !!status?.download_in_progress;
  useEffect(() => {
    if (!desktop || !downloadingNow) return;
    const timer = window.setInterval(() => {
      void getDictationStatus()
        .then((latest) => {
          if (latest) publish(latest);
        })
        .catch(() => undefined);
    }, 1000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [desktop, downloadingNow]);

  // Starting the download returns at once; the transfer itself is reported by the progress and finish events.
  const download = async () => {
    setError(null);
    setProgress({ downloaded_bytes: 0, total_bytes: status?.model_bytes || 0 });
    try {
      publish(await downloadDictationModel());
    } catch (downloadError) {
      setError(voiceError(downloadError, t));
      const latest = await getDictationStatus();
      if (latest) publish(latest);
    }
  };

  const cancelDownload = async () => {
    await cancelDictationDownload().catch(() => undefined);
  };

  const repair = async () => {
    setError(null);
    try {
      publish(await deleteDictationModel());
      await download();
    } catch (repairError) {
      setError(voiceError(repairError, t));
    }
  };

  const remove = async () => {
    if (!window.confirm(t("settings.voice.deleteConfirm"))) return;
    setError(null);
    try {
      publish(await deleteDictationModel());
      setTestTranscript("");
      setProgress(null);
    } catch (deleteError) {
      setError(voiceError(deleteError, t));
    }
  };

  const toggleTest = async () => {
    if (!status?.supported || !status.model_verified) return;
    setError(null);
    try {
      if (status.recording) {
        setPhase("transcribing");
        const transcript = (await stopDictation()).trim();
        setTestTranscript(transcript);
        if (!transcript) throw new Error(t("settings.voice.noSpeech"));
        publish(await markDictationPassed());
      } else {
        setTestTranscript("");
        setPhase("testing");
        publish(await startDictation());
      }
    } catch (testError) {
      setError(voiceError(testError, t));
      const latest = await getDictationStatus();
      if (latest) publish(latest);
    } finally {
      setPhase("idle");
    }
  };

  const downloading = !!status?.download_in_progress;
  const progressTotal = progress?.total_bytes || status?.model_bytes || 1;
  const progressPercent = Math.min(100, Math.round(((progress?.downloaded_bytes || 0) / progressTotal) * 100));
  const ready = !!status?.supported && !!status?.model_verified && !!status?.test_passed;
  const checking = phase === "transcribing"; // mic test: stopping + transcribing

  return (
    <section>
      <PanelHead
        title={t("settings.voice.title")}
        sub={t("settings.voice.sub")}
      />

      {!desktop ? (
        <div className={CARD + " p-4 text-[13px] text-muted"}>{t("settings.voice.desktopOnly")}</div>
      ) : (
        <div className="space-y-4">
          <div className="rounded-xl border border-ok-line bg-ok-soft/70 px-4 py-3 text-[12.5px] text-ok">
            <span className="font-medium">{t("settings.voice.privateByDesign")}</span> {t("settings.voice.privateByDesignSub")}
          </div>

          <div className={CARD}>
            <div className="p-4 flex items-start gap-3">
              <Icon name="code" size={18} className="text-primary mt-0.5" />
              <div className="min-w-0 flex-1">
                <div className="text-[13.5px] font-medium">{t("settings.voice.thisDevice")}</div>
                <div className="text-[12px] text-muted mt-1">{status?.device_summary || t("settings.voice.checkingCompat")}</div>
                {status?.compatibility_reason && <div className="text-[12px] text-danger mt-1.5">{status.compatibility_reason}</div>}
              </div>
              {status && (
                <span className={"text-[11.5px] px-2 py-1 rounded-full " + (status.supported ? "bg-ok-soft text-ok" : "bg-danger-soft text-danger")}>
                  {status.supported ? t("settings.voice.compatible") : t("settings.voice.unsupported")}
                </span>
              )}
            </div>
            <div className="border-t border-line bg-bg/50 px-4 py-3 grid grid-cols-2 gap-3 text-[12px] text-muted">
              <div><span className="block text-ink font-medium">{t("settings.voice.reqMac")}</span>{t("settings.voice.reqMacSpec")}</div>
              <div><span className="block text-ink font-medium">{t("settings.voice.reqWin")}</span>{t("settings.voice.reqWinSpec")}</div>
              <div><span className="block text-ink font-medium">{t("settings.voice.memory")}</span>{t("settings.voice.memoryReq")}</div>
              <div><span className="block text-ink font-medium">{t("settings.voice.processor")}</span>{t("settings.voice.cpuReq")}</div>
            </div>
          </div>

          <div className={CARD}>
            <div className="p-4 flex items-center gap-3">
              <div className="w-9 h-9 rounded-lg bg-selected text-primary grid place-items-center font-semibold">S</div>
              <div className="min-w-0 flex-1">
              <div className="text-[13.5px] font-medium">
                {t("settings.voice.modelName")}
              </div>
                <div className="text-[12px] text-muted mt-0.5">
                  {status?.model_verified ? t("settings.voice.installedAndVerified", { size: formatBytes(status.model_bytes) }) : t("settings.voice.localVoiceModel", { size: formatBytes(status?.model_bytes || 239_549_735) })}
                </div>
              </div>
              {status?.model_verified ? (
                <>
                  <span className="text-[11.5px] px-2 py-1 rounded-full bg-ok-soft text-ok">{t("settings.voice.verified")}</span>
                  <button className={BTN_BORDERED} onClick={() => void repair()}>{t("common.retry")}</button>
                  <button className="text-[12px] text-danger px-2 py-2" onClick={() => void remove()}>{t("common.delete")}</button>
                </>
              ) : downloading ? (
                <button className={BTN_BORDERED} onClick={() => void cancelDownload()}>{t("common.cancel")}</button>
              ) : phase === "verifying" ? (
                <span className="text-[12px] text-muted">{t("settings.voice.verifying")}</span>
              ) : (
                <button className={BTN_ACCENT} disabled={!status?.supported} onClick={() => void download()}>{t("settings.voice.downloadModel")}</button>
              )}
            </div>
            {downloading && (
              <div className="border-t border-line px-4 py-3">
                <div className="h-1.5 rounded-full bg-line overflow-hidden"><div className="h-full bg-primary transition-all" style={{ width: `${progressPercent}%` }} /></div>
                <div className="mt-1.5 text-[11.5px] text-muted flex"><span>{formatBytes(progress?.downloaded_bytes || 0)} {t("settings.voice.of")} {formatBytes(progressTotal)}</span><span className="ml-auto">{progressPercent}%</span></div>
                <div className="mt-1 text-[11.5px] text-faint">{t("settings.voice.backgroundHint")}</div>
              </div>
            )}
          </div>

          <div className={CARD}>
            <div className="p-4 flex items-center gap-3">
              <Icon name="mic" size={18} className={ready ? "text-ok" : "text-muted"} />
              <div className="min-w-0 flex-1">
                <div className="text-[13.5px] font-medium">{t("settings.voice.micTest")}</div>
                <div className="text-[12px] text-muted mt-0.5">
                  {ready ? t("settings.voice.micReady") : t("settings.voice.micHint")}
                </div>
              </div>
              {ready && <span className="text-[11.5px] px-2 py-1 rounded-full bg-ok-soft text-ok inline-flex items-center gap-1"><Icon name="check" size={11} />{t("settings.voice.ready")}</span>}
              <button className={BTN_BORDERED} disabled={!status?.supported || !status?.model_verified || checking} onClick={() => void toggleTest()}>
                {checking ? t("settings.voice.transcribing") : status?.recording ? t("settings.voice.stopAndCheck") : ready ? t("settings.voice.testAgain") : t("settings.voice.testMic")}
              </button>
            </div>
            {status?.recording && <div className="border-t border-line px-4 py-3 text-[12px] text-primary" role="status">● {t("settings.voice.listening")}</div>}
            {testTranscript && <div className="border-t border-line bg-bg/50 px-4 py-3 text-[13px]">“{testTranscript}”</div>}
          </div>

          {error && <div role="alert" className="rounded-lg border border-danger-soft bg-danger-soft px-3 py-2.5 text-[12px] text-danger">{error}</div>}
        </div>
      )}
    </section>
  );
}

// -- Appearance + app behaviour ------------------------------------------------
function AppearanceSection() {
  const [theme, setTheme] = useThemePref();
  const [isAutostart, setAuto] = useState(false);
  const [shouldKeepAwake, setKeep] = useState(false);
  const { locale, setLocale, t } = useI18n();
  const desktop = isTauri();

  useEffect(() => {
    if (isTauri()) {
      getAutostart().then((v) => setAuto(!!v));
      getKeepAwake().then((v) => setKeep(!!v));
    }
  }, []);

  const toggleAuto = async (isEnabled: boolean) => setAuto(!!(await setAutostart(isEnabled)));
  const toggleKeep = async (isEnabled: boolean) => setKeep(!!(await setKeepAwake(isEnabled)));
  const runSetupAgain = async () => {
    await setOnboarded(false);
    window.dispatchEvent(new CustomEvent("delta:open-onboarding"));
  };
  const langOptions: Locale[] = ["zh-CN", "en-US"];

  return (
    <section>
      <PanelHead title={t("settings.general.title")} sub={t("settings.general.sub")} />

      <div className={CARD + " p-4 mb-4"}>
        <div className={FIELD_LABEL}>{t("settings.general.theme")}</div>
        <div className="seg mt-2.5" role="radiogroup" aria-label={t("common.appearance")}>
          {(["light", "dark", "auto"] as const).map((p) => (
            <button key={p} className={p === theme ? "active" : ""} onClick={() => setTheme(p)}>
              {p === "light" ? t("common.light") : p === "dark" ? t("common.dark") : t("common.auto")}
            </button>
          ))}
        </div>
        <div className={FIELD_HELP}>{t("settings.general.autoFollows")}</div>
      </div>

      <div className={CARD + " p-4 mb-4"}>
        <div className={FIELD_LABEL}>{t("settings.general.language")}</div>
        <div className="seg mt-2.5" role="radiogroup" aria-label={t("settings.general.language")}>
          {langOptions.map((l) => (
            <button
              key={l}
              className={l === locale ? "active" : ""}
              onClick={() => setLocale(l)}
            >
              {l === "zh-CN" ? ZH_CN_LABEL : "English"}
            </button>
          ))}
        </div>
        <div className={FIELD_HELP}>{t("settings.general.languageSub")}</div>
      </div>

      <SidebarCard />

      <ContextBarCard />

      <FilesCard />

      <TrustedWorkspacesCard />

      {desktop && (
        <div className={CARD + " p-4"}>
          <div className={FIELD_LABEL + " mb-2.5"}>{t("settings.general.alwaysOn")}</div>
          <label className="flex items-start gap-3 py-2">
            <input type="checkbox" className="mt-0.5" checked={isAutostart} onChange={(e) => toggleAuto(e.target.checked)} />
            <span>
              <span className="block text-[13px] text-ink">{t("settings.general.startAtLogin")}</span>
              <span className="block text-[12px] text-muted">{t("settings.general.startAtLoginSub")}</span>
            </span>
          </label>
          <label className="flex items-start gap-3 py-2">
            <input type="checkbox" className="mt-0.5" checked={shouldKeepAwake} onChange={(e) => toggleKeep(e.target.checked)} />
            <span>
              <span className="block text-[13px] text-ink">{t("settings.general.keepAwake")}</span>
              <span className="block text-[12px] text-muted">{t("settings.general.keepAwakeSub")}</span>
            </span>
          </label>
        </div>
      )}

      {/* One card for the app-lifecycle actions (UX-021): the onboarding replay (§24 —
          every build, the browser dev shell runs the same first-run flow) and, on
          desktop, the manual update check (launch also checks automatically). */}
      <div className={CARD + " p-4 mt-4"}>
        <div className={FIELD_LABEL + " mb-2"}>{t("settings.general.setupUpdates")}</div>
        <div className="flex items-center gap-2">
          <button className={BTN_BORDERED} onClick={runSetupAgain}>
            {t("settings.general.runSetupAgain")}
          </button>
          {desktop && UPDATER_FEED_PUBLISHED && <UpdateInline />}
        </div>
        <div className={FIELD_HELP}>{t("settings.general.setupUpdatesHelp")}</div>
      </div>
    </section>
  );
}

function TrustedWorkspacesCard() {
  const { t } = useI18n();
  const [workspaces, setWorkspaces] = useState<WorkspaceCommandTrust[] | null>(null);
  // A failed read must not say "no trusted workspaces": it is a security list, so it says it could not load.
  const [hasFailed, setFailed] = useState(false);

  const refresh = () => {
    setFailed(false);
    return getTrustedWorkspaces()
      .then(setWorkspaces)
      .catch(() => setFailed(true));
  };

  useEffect(() => {
    refresh();
  }, []);

  const revoke = async (path: string) => {
    if (!window.confirm(t("settings.workspace.confirmRevoke", { path }))) return;
    await setWorkspaceTrusted(path, false);
    refresh();
  };

  return (
    <div className={CARD + " p-4 mb-4"} data-testid="trusted-workspaces-card">
      <div className={FIELD_LABEL}>{t("settings.workspace.title")}</div>
      <div className={FIELD_HELP}>{t("settings.workspace.help")}</div>
      {hasFailed && workspaces === null ? (
        <div className="mt-3">
          <LoadError what={t("state.what.trustedWorkspaces", undefined, "trusted workspaces")} onRetry={refresh} />
        </div>
      ) : workspaces === null ? (
        <div className="text-[12px] text-muted mt-3">{t("common.loading")}</div>
      ) : workspaces.length === 0 ? (
        <div className="text-[12px] text-muted mt-3">{t("settings.workspace.empty")}</div>
      ) : (
        <div className="mt-3 divide-y divide-line">
          {workspaces.map((workspace) => (
            <div key={workspace.workspace} className="py-2.5 flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <div className="text-[12.5px] text-ink break-all">{workspace.workspace}</div>
                <div className="text-[11.5px] text-muted mt-0.5">
                  {workspace.requested_commands.length
                    ? t(
                        "settings.workspace.allowanceCount",
                        { n: workspace.requested_commands.length },
                        `${workspace.requested_commands.length} project command allowance${workspace.requested_commands.length === 1 ? "" : "s"}`,
                      )
                    : t("settings.workspace.noAllowances")}
                  {!workspace.exists ? t("settings.workspace.folderUnavailable") : ""}
                </div>
              </div>
              <button
                className="text-[12px] text-danger px-2 py-1"
                onClick={() => void revoke(workspace.workspace)}
              >
                {t("settings.workspace.revoke")}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function UpdateInline() {
  const { t } = useI18n();
  const [state, setState] = useState<"idle" | "checking" | "none" | "found" | "installing" | "error">("idle");
  const [version, setVersion] = useState("");

  const check = async () => {
    setState("checking");
    try {
      const u = await checkForUpdate();
      if (u) {
        setVersion(u.version);
        setState("found");
      } else {
        setState("none");
      }
    } catch {
      setState("error");
    }
  };

  const install = async () => {
    setState("installing");
    try {
      await installUpdate(); // success restarts the app
    } catch {
      setState("error");
    }
  };

  return (
    <span className="inline-flex items-center gap-2.5">
      {state === "found" ? (
        <button className={BTN_BORDERED} onClick={install} data-testid="settings-update-install">
          {t("settings.update.toLatest", { version })}
        </button>
      ) : (
        <button
          className={BTN_BORDERED}
          onClick={check}
          disabled={state === "checking" || state === "installing"}
          data-testid="settings-update-check"
        >
          {state === "checking" ? t("settings.update.checking") : t("settings.update.check")}
        </button>
      )}
      {(state === "none" || state === "error" || state === "installing") && (
        <span className="text-[12px] text-muted">
          {state === "none"
            ? t("settings.update.latest")
            : state === "error"
              ? t("settings.update.checkFailed")
              : t("settings.update.downloading")}
        </span>
      )}
    </span>
  );
}

// Telemetry/Privacy card removed for this release (owner ask 2026-07-22); the
// setCloudTelemetry API stays for a future opt-out surface.

// -- Sidebar density -------------------------------------------------------------
// -- Token savings (PDF attachments; owner ask, 2026-07-17) ---------------------
// Attachments replay with EVERY turn, so a big PDF quietly multiplies token spend.
// This card is the attachment dial: attach thresholds + the fallback for models
// without native PDF support. (Long-history spend is handled by auto-compaction —
// the CompactionCard below, OPE-27.)
function TokenSavingsCard() {
  const { t } = useI18n();
  const [pdf, setPdf] = useState<PdfSettings | null>(null);
  // A failed read must not show the built-in limits as the saved ones (editing them would overwrite the real values).
  const [hasFailed, setFailed] = useState(false);

  const load = () => {
    setFailed(false);
    getSettingsStrict()
      .then((s) =>
        setPdf({
          pdf_fallback: s.pdf_fallback || "text",
          pdf_max_pages: s.pdf_max_pages || 20,
          pdf_max_mb: s.pdf_max_mb || 10,
        }),
      )
      .catch(() => setFailed(true));
  };
  useEffect(load, []);

  const save = async (patch: Partial<PdfSettings>) => {
    setPdf((p) => (p ? { ...p, ...patch } : p));
    await setPdfSettings(patch);
  };

  if (hasFailed && !pdf) return <LoadError what={t("state.what.settings", undefined, "these settings")} onRetry={load} />;
  if (!pdf) return null;
  return (
    <div className={CARD + " p-4 mb-4"} data-testid="token-savings-card">
      <div className={FIELD_LABEL}>{t("settings.pdf.title")}</div>
      <div className={FIELD_HELP}>{t("settings.pdf.help")}</div>

      <div className="mt-3 text-[13px] text-ink">{t("settings.pdf.fallbackLabel")}</div>
      <div className="seg mt-2" role="radiogroup" aria-label={t("settings.pdf.fallbackAria")} data-testid="pdf-fallback">
        <button
          className={pdf.pdf_fallback === "text" ? "active" : ""}
          onClick={() => save({ pdf_fallback: "text" })}
        >
          {t("settings.pdf.extractText")}
        </button>
        <button
          className={pdf.pdf_fallback === "images" ? "active" : ""}
          onClick={() => save({ pdf_fallback: "images" })}
        >
          {t("settings.pdf.sendImages")}
        </button>
      </div>
      <div className={FIELD_HELP}>{t("settings.pdf.help2")}</div>

      <div className="mt-3 flex items-center gap-5">
        <label className="flex items-center gap-2.5">
          <span className="text-[13px] text-ink">{t("settings.pdf.maxPages")}</span>
          <input
            type="number"
            min={1}
            max={100}
            value={pdf.pdf_max_pages}
            data-testid="pdf-max-pages"
            className="w-16 px-2 py-1.5 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary"
            onChange={(e) => save({ pdf_max_pages: Math.max(1, Math.min(Number(e.target.value) || 20, 100)) })}
          />
        </label>
        <label className="flex items-center gap-2.5">
          <span className="text-[13px] text-ink">{t("settings.pdf.maxSize")}</span>
          <input
            type="number"
            min={1}
            max={10}
            value={pdf.pdf_max_mb}
            data-testid="pdf-max-mb"
            className="w-16 px-2 py-1.5 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary"
            onChange={(e) => save({ pdf_max_mb: Math.max(1, Math.min(Number(e.target.value) || 10, 10)) })}
          />
          <span className="text-[12.5px] text-muted">MB</span>
        </label>
      </div>
      <div className={FIELD_HELP}>{t("settings.pdf.help3")}</div>
    </div>
  );
}

// -- Web search engines ---------------------------------------------------------------------
// Like a browser's search engine list: Google, Bing and DuckDuckGo work without a key; any of them
// (and Brave, Tavily) can take a key; the user can add engines of their own (an address with %s) and
// choose their order. Keys go to the credential store and are never read back: the card only knows
// that one exists.
interface Draft extends SearchEngine {
  /** Typed in this session; sent on save. */
  api_key?: string;
  cx?: string;
  clear_key?: boolean;
  /** Added in this session, not saved yet. */
  fresh?: boolean;
}

const toEdit = (d: Draft): SearchEngineEdit => ({
  ...(d.fresh ? {} : { id: d.id }),
  name: d.name,
  enabled: d.enabled,
  ...(d.builtin ? {} : { template: d.template, auth_header: d.auth_header }),
  ...(d.api_key ? { api_key: d.api_key } : {}),
  ...(d.cx ? { cx: d.cx } : {}),
  ...(d.clear_key ? { clear_key: true } : {}),
});

// Off by default: approvals stay in the conversation. On: the dialog (ApprovalModal) also takes
// every other approval, not only the outbound and connector ones.
function ApprovalDialogCard() {
  const { t } = useI18n();
  const [isOn, setOn] = useImportantOpsPref();
  return (
    <div className={CARD + " p-4 flex items-start justify-between gap-4"}>
      <div className="min-w-0">
        <div className={FIELD_LABEL}>{t("settings.approvalDialog.title")}</div>
        <p className="text-[12.5px] text-muted mt-1">{t("settings.approvalDialog.sub")}</p>
      </div>
      <Toggle checked={isOn} onChange={setOn} title={t("settings.approvalDialog.title")} />
    </div>
  );
}

export function WebSearchCard() {
  const { t } = useI18n();
  const [drafts, setDrafts] = useState<Draft[] | null>(null);
  const [hasFailed, setFailed] = useState(false);
  const [isDirty, setDirty] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setBusy] = useState(false);
  const [tests, setTests] = useState<Record<string, string>>({});
  const [isAdding, setAdding] = useState(false);
  const [form, setForm] = useState({ name: "", template: "", header: "", key: "" });

  const load = () => {
    setFailed(false);
    readSearchEngines()
      .then((engines) => {
        setDrafts(engines);
        setDirty(false);
      })
      .catch(() => setFailed(true));
  };
  useEffect(load, []);

  if (hasFailed && !drafts) return <LoadError what={t("settings.search.title")} onRetry={load} />;
  if (!drafts) return null;

  const change = (id: string, patch: Partial<Draft>) => {
    setDrafts((list) => list && list.map((d) => (d.id === id ? { ...d, ...patch } : d)));
    setDirty(true);
  };
  const move = (index: number, by: -1 | 1) => {
    setDrafts((list) => {
      if (!list || index + by < 0 || index + by >= list.length) return list;
      const next = [...list];
      [next[index], next[index + by]] = [next[index + by], next[index]];
      return next;
    });
    setDirty(true);
  };
  // The default is the first engine that is on. Setting one as default turns it on and moves it to the top.
  const makeDefault = (id: string) => {
    setDrafts((list) => {
      if (!list) return list;
      const chosen = list.find((d) => d.id === id);
      if (!chosen) return list;
      return [{ ...chosen, enabled: true }, ...list.filter((d) => d.id !== id)];
    });
    setDirty(true);
  };
  const remove = (id: string) => {
    setDrafts((list) => list && list.filter((d) => d.id !== id));
    setOpen(null);
    setDirty(true);
  };
  const addEngine = () => {
    const id = `new-${Date.now()}`;
    setDrafts((list) => [
      ...(list ?? []),
      {
        id, name: form.name.trim(), builtin: false, enabled: true, needs_key: false, accepts_key: true,
        has_key: false, has_cx: false, template: form.template.trim(), auth_header: form.header.trim(),
        ...(form.key.trim() ? { api_key: form.key.trim() } : {}), fresh: true,
      },
    ]);
    setForm({ name: "", template: "", header: "", key: "" });
    setAdding(false);
    setDirty(true);
  };
  const save = async () => {
    setBusy(true);
    setError(null);
    const result = await saveSearchEngines(drafts.map(toEdit)).catch(() => ({
      ok: false as const,
      error: t("settings.search.saveFailed"),
    }));
    setBusy(false);
    if (!result.ok || !result.engines) {
      setError(result.error || t("settings.search.saveFailed"));
      return;
    }
    setDrafts(result.engines);
    setDirty(false);
    setTests({});
  };
  const runTest = async (id: string) => {
    setTests((cur) => ({ ...cur, [id]: "…" }));
    const out: { ok: boolean; count?: number; first?: string; error?: string } = await probeSearchEngine(id).catch(
      () => ({ ok: false, error: "" }),
    );
    setTests((cur) => ({
      ...cur,
      [id]: out.ok
        ? t("settings.search.testOk", { n: out.count ?? 0, first: out.first ?? "" })
        : `${t("settings.search.testFailed")} ${out.error ?? ""}`.trim(),
    }));
  };
  const formOk = form.name.trim() !== "" && /^https?:\/\/.+/.test(form.template.trim()) && form.template.split("%s").length === 2;

  return (
    <div className={CARD + " p-4"} data-testid="web-search-card">
      <div className={FIELD_LABEL}>{t("settings.search.title")}</div>
      <div className={FIELD_HELP}>{t("settings.search.help")}</div>
      <ul className="mt-3 divide-y divide-line border border-line rounded-lg" data-testid="search-engines">
        {drafts.map((d, index) => {
          const isDefault = drafts.find((x) => x.enabled)?.id === d.id;
          const canBeDefault = !d.needs_key || d.has_key || !!d.api_key;
          const keyed = d.has_key && !d.clear_key;
          const status = d.needs_key && !keyed && !d.api_key ? t("settings.search.needsKey") : keyed || d.api_key ? t("settings.search.hasKey") : "";
          return (
            <li key={d.id} className="px-3 py-2" data-testid={`search-engine-${d.id}`}>
              <div className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={d.enabled}
                  aria-label={d.name}
                  data-testid={`search-enabled-${d.id}`}
                  onChange={(e) => change(d.id, { enabled: e.target.checked })}
                />
                <span className="text-[13px] font-medium text-ink truncate">{d.name}</span>
                {isDefault && <span className="text-[11px] font-medium text-primary" data-testid={`search-default-${d.id}`}>{t("settings.search.default")}</span>}
                {!d.builtin && <span className="text-[11px] text-faint">{t("settings.search.custom")}</span>}
                {status && <span className="text-[11.5px] text-muted">{status}</span>}
                <span className="ml-auto flex items-center gap-1">
                  {!isDefault && canBeDefault && !d.fresh && (
                    <button className="text-[12px] px-1.5 text-muted hover:text-ink" onClick={() => makeDefault(d.id)} data-testid={`search-makedefault-${d.id}`}>
                      {t("settings.search.makeDefault")}
                    </button>
                  )}
                  <button className="text-[12px] px-1.5 text-muted hover:text-ink" aria-label={t("settings.search.moveUp")} disabled={index === 0} onClick={() => move(index, -1)} data-testid={`search-up-${d.id}`}>↑</button>
                  <button className="text-[12px] px-1.5 text-muted hover:text-ink" aria-label={t("settings.search.moveDown")} disabled={index === drafts.length - 1} onClick={() => move(index, 1)} data-testid={`search-down-${d.id}`}>↓</button>
                  <button className="text-[12px] px-2 text-primary" onClick={() => setOpen(open === d.id ? null : d.id)} data-testid={`search-edit-${d.id}`}>
                    {t("settings.search.edit")}
                  </button>
                </span>
              </div>
              {open === d.id && (
                <div className="mt-2 pl-6 space-y-2">
                  {!d.builtin && (
                    <>
                      <input className={INPUT + " w-full"} value={d.name} placeholder={t("settings.search.namePlaceholder")} onChange={(e) => change(d.id, { name: e.target.value })} />
                      <input className={INPUT + " w-full"} value={d.template} placeholder="https://example.com/search?q=%s" data-testid={`search-template-${d.id}`} onChange={(e) => change(d.id, { template: e.target.value })} />
                      <input className={INPUT + " w-full"} value={d.auth_header} placeholder={t("settings.search.headerPlaceholder")} onChange={(e) => change(d.id, { auth_header: e.target.value })} />
                    </>
                  )}
                  {d.accepts_key && (
                    <input
                      type="password" autoComplete="off" spellCheck={false} className={INPUT + " w-full"}
                      value={d.api_key ?? ""} data-testid={`search-key-${d.id}`}
                      placeholder={keyed ? t("settings.search.keySaved") : t("settings.search.keyPlaceholder")}
                      onChange={(e) => change(d.id, { api_key: e.target.value, clear_key: false })}
                    />
                  )}
                  {d.id === "google" && (
                    <input
                      className={INPUT + " w-full"} value={d.cx ?? ""} data-testid="search-cx-google"
                      placeholder={d.has_cx ? t("settings.search.keySaved") : t("settings.search.cxPlaceholder")}
                      onChange={(e) => change(d.id, { cx: e.target.value })}
                    />
                  )}
                  <div className="flex items-center gap-3 text-[12px]">
                    {keyed && <button className="text-danger" onClick={() => change(d.id, { clear_key: true, api_key: "" })} data-testid={`search-clearkey-${d.id}`}>{t("settings.search.removeKey")}</button>}
                    {!d.builtin && <button className="text-danger" onClick={() => remove(d.id)} data-testid={`search-remove-${d.id}`}>{t("settings.search.removeEngine")}</button>}
                    <button className="text-primary disabled:opacity-40" disabled={isDirty || d.fresh || (d.needs_key && !d.has_key)} title={isDirty ? t("settings.search.saveFirst") : undefined} onClick={() => runTest(d.id)} data-testid={`search-test-${d.id}`}>
                      {t("settings.search.test")}
                    </button>
                    {tests[d.id] && <span className="text-muted" data-testid={`search-test-result-${d.id}`}>{tests[d.id]}</span>}
                  </div>
                </div>
              )}
            </li>
          );
        })}
      </ul>

      {isAdding ? (
        <div className="mt-3 space-y-2" data-testid="search-add-form">
          <input className={INPUT + " w-full"} value={form.name} placeholder={t("settings.search.namePlaceholder")} data-testid="search-add-name" onChange={(e) => setForm({ ...form, name: e.target.value })} />
          <input className={INPUT + " w-full"} value={form.template} placeholder="https://example.com/search?q=%s" data-testid="search-add-template" onChange={(e) => setForm({ ...form, template: e.target.value })} />
          <div className={FIELD_HELP}>{t("settings.search.addHelp")}</div>
          <input className={INPUT + " w-full"} value={form.header} placeholder={t("settings.search.headerPlaceholder")} onChange={(e) => setForm({ ...form, header: e.target.value })} />
          <input type="password" autoComplete="off" className={INPUT + " w-full"} value={form.key} placeholder={t("settings.search.keyOptional")} onChange={(e) => setForm({ ...form, key: e.target.value })} />
          <div className="flex gap-2">
            <button className={BTN_ACCENT} disabled={!formOk} onClick={addEngine} data-testid="search-add-confirm">{t("settings.search.add")}</button>
            <button className={BTN_BORDERED} onClick={() => setAdding(false)}>{t("common.cancel", undefined, "Cancel")}</button>
          </div>
        </div>
      ) : (
        <button className="mt-3 text-[12.5px] text-primary" onClick={() => setAdding(true)} data-testid="search-add">
          + {t("settings.search.addEngine")}
        </button>
      )}

      <div className="mt-3 flex items-center gap-3">
        <button className={BTN_ACCENT} disabled={!isDirty || isBusy} onClick={save} data-testid="search-save">
          {t("common.save", undefined, "Save")}
        </button>
        {error && (
          <span className="text-[12px] text-danger" role="alert" data-testid="search-error">
            {error}
          </span>
        )}
      </div>
    </div>
  );
}

// -- Context compaction (OPE-27) ------------------------------------------------
// Long sessions are summarized automatically when they approach the model's context
// limit, so work continues instead of hitting a raw provider error. Two spec'd
// overrides (trigger % + token cap) and the summarizer-model pin — nothing more.
function CompactionCard() {
  const { t } = useI18n();
  const [cfg, setCfg] = useState<CompactionSettings | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [labels, setLabels] = useState<Record<string, string>>({});
  // A failed read must not show the built-in thresholds as the saved ones.
  const [hasFailed, setFailed] = useState(false);

  const load = () => {
    setFailed(false);
    getSettingsStrict()
      .then((s) => {
        setCfg({
          compaction_threshold_pct: s.compaction_threshold_pct ?? 0.8,
          compaction_cap_tokens: s.compaction_cap_tokens ?? 250_000,
          compaction_model: s.compaction_model ?? "",
        });
        setModels(s.models || []);
        setLabels(s.model_labels || {});
      })
      .catch(() => setFailed(true));
  };
  useEffect(load, []);

  const save = async (patch: Partial<CompactionSettings>) => {
    setCfg((p) => (p ? { ...p, ...patch } : p));
    await setCompactionSettings(patch);
  };

  if (hasFailed && !cfg) return <LoadError what={t("state.what.settings", undefined, "these settings")} onRetry={load} />;
  if (!cfg) return null;
  const modelLabel = (id: string) => labels[id]?.split(" · ")[0] || id;
  return (
    <div className={CARD + " p-4 mb-4"} data-testid="compaction-card">
      <div className={FIELD_LABEL}>{t("settings.compaction.title")}</div>
      <div className={FIELD_HELP}>{t("settings.compaction.help")}</div>

      <div className="mt-3 flex items-center gap-5 flex-wrap">
        <label className="flex items-center gap-2.5">
          <span className="text-[13px] text-ink">{t("settings.compaction.atLabel")}</span>
          <input
            type="number"
            min={10}
            max={95}
            value={Math.round(cfg.compaction_threshold_pct * 100)}
            data-testid="compaction-threshold"
            className="w-16 px-2 py-1.5 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary"
            onChange={(e) =>
              save({
                compaction_threshold_pct:
                  Math.max(10, Math.min(Number(e.target.value) || 80, 95)) / 100,
              })
            }
          />
          <span className="text-[12.5px] text-muted">{t("settings.compaction.pctUnit")}</span>
        </label>
        <label className="flex items-center gap-2.5">
          <span className="text-[13px] text-ink">{t("settings.compaction.orAt")}</span>
          <input
            type="number"
            min={10_000}
            max={2_000_000}
            step={10_000}
            value={cfg.compaction_cap_tokens}
            data-testid="compaction-cap"
            className="w-28 px-2 py-1.5 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary"
            onChange={(e) =>
              save({
                compaction_cap_tokens: Math.max(
                  10_000,
                  Math.min(Number(e.target.value) || 250_000, 2_000_000),
                ),
              })
            }
          />
          <span className="text-[12.5px] text-muted">{t("settings.compaction.tokenUnit")}</span>
        </label>
      </div>
      <div className={FIELD_HELP}>{t("settings.compaction.capHelp")}</div>

      <div className="mt-3 flex items-center gap-2.5">
        <span className="text-[13px] text-ink">{t("settings.compaction.modelLabel")}</span>
        <select
          value={cfg.compaction_model}
          data-testid="compaction-model"
          className="px-2 py-1.5 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary"
          onChange={(e) => save({ compaction_model: e.target.value })}
        >
          <option value="">{t("settings.compaction.sessionModel")}</option>
          {models.map((m) => (
            <option key={m} value={m}>
              {modelLabel(m)}
            </option>
          ))}
        </select>
      </div>
      <div className={FIELD_HELP}>{t("settings.compaction.modelHelp")}</div>
    </div>
  );
}

// -- Composer: context-window bar (owner ask 2026-07-30) ------------------------
// The chip's bar is context-window occupancy; the session total (unbounded) lives in
// the popover. Some people would rather not watch a meter at all, hence the toggle.
function ContextBarCard() {
  const { t } = useI18n();
  const [shown, setShown] = useState<boolean | null>(null);
  // A failed read must not show the switch as off (that would be a guess, and a click would save it).
  const [hasFailed, setFailed] = useState(false);

  const load = () => {
    setFailed(false);
    getSettingsStrict()
      .then((s) => setShown(s.context_bar === true))
      .catch(() => setFailed(true));
  };
  useEffect(load, []);

  const save = async (isShown: boolean) => {
    setShown(isShown);
    await setContextBar(isShown);
  };

  if (hasFailed && shown === null) return <LoadError what={t("state.what.settings", undefined, "these settings")} onRetry={load} />;
  if (shown === null) return null;
  return (
    <div className={CARD + " p-4 mb-4"} data-testid="context-bar-card">
      <div className={FIELD_LABEL}>{t("settings.composer.title")}</div>
      <label className="flex items-start gap-3 py-2">
        <input
          type="checkbox"
          className="mt-0.5"
          data-testid="context-bar-toggle"
          checked={shown}
          onChange={(e) => save(e.target.checked)}
        />
        <span>
          <span className="block text-[13px] text-ink">{t("settings.composer.showBar")}</span>
          <span className="block text-[12px] text-muted">{t("settings.composer.showBarHelp")}</span>
        </span>
      </label>
    </div>
  );
}

// Which route outbound requests take. One setting for every door, because a network
// that needs a proxy needs it for the model call AND the web search that ran with it:
// half a turn proxied is the bug this replaces.
function NetworkProxyCard() {
  const { t } = useI18n();
  const [settings, setSettings] = useState<ProxySettings | null>(null);
  const [draft, setDraft] = useState<ProxySettings | null>(null);
  const [error, setError] = useState("");
  const [isSaved, setSaved] = useState(false);
  const [hasFailed, setFailed] = useState(false);

  const load = () => {
    setFailed(false);
    getSettingsStrict()
      .then((s) => {
        const proxy: ProxySettings = {
          mode: s.proxy?.mode ?? "direct",
          http: s.proxy?.http ?? "",
          https: s.proxy?.https ?? "",
          no_proxy: s.proxy?.no_proxy ?? "",
          system_detected: s.proxy?.system_detected ?? null,
        };
        setSettings(proxy);
        setDraft(proxy);
      })
      .catch(() => setFailed(true));
  };
  useEffect(load, []);

  if (hasFailed && !settings) {
    return <LoadError what={t("state.what.settings", undefined, "these settings")} onRetry={load} />;
  }
  if (!draft) return null;

  const patch = (next: Partial<ProxySettings>) => {
    setDraft({ ...draft, ...next });
    setSaved(false);
  };

  // The mode change is saved immediately (it is the whole decision); the addresses
  // wait for Save so a half-typed host:port is never persisted.
  const chooseMode = async (mode: ProxyMode) => {
    patch({ mode });
    await persist({ mode });
  };

  const persist = async (fields: Partial<ProxySettings>) => {
    setError("");
    const out = await setProxySettings(fields).catch(() => null);
    if (!out || out.ok === false) {
      setError(t("settings.proxy.error"));
      return;
    }
    if (out.proxy) {
      setSettings(out.proxy);
      setDraft({ ...draft, ...out.proxy, ...fields });
    }
    setSaved(true);
  };

  const isManual = draft.mode === "manual";
  const detected = draft.system_detected;
  return (
    <div className={CARD + " p-4 mb-4"} data-testid="proxy-card">
      <div className={FIELD_LABEL}>{t("settings.proxy.title")}</div>
      <div className={FIELD_HELP}>{t("settings.proxy.help")}</div>
      <div role="radiogroup" className="flex flex-col gap-2 mt-3">
        {(["direct", "system", "manual"] as ProxyMode[]).map((mode) => (
          <label
            key={mode}
            data-testid={`proxy-mode-${mode}`}
            className={
              "flex items-start gap-2.5 rounded-lg border px-3 py-2.5 cursor-pointer " +
              (draft.mode === mode
                ? "border-primary bg-bg"
                : "border-line hover:border-line-strong")
            }
          >
            <input
              type="radio"
              name="proxy-mode"
              className="mt-1"
              checked={draft.mode === mode}
              onChange={() => chooseMode(mode)}
            />
            <span>
              <span className="block text-[13px] font-medium text-ink">
                {t(`settings.proxy.${mode}`)}
              </span>
              <span className="block text-[12px] text-muted mt-0.5">
                {t(`settings.proxy.${mode}Help`)}
              </span>
            </span>
          </label>
        ))}
      </div>

      {draft.mode === "system" && (
        <div className={FIELD_HELP} data-testid="proxy-detected">
          {detected
            ? t("settings.proxy.detected", { value: detected })
            : t("settings.proxy.detectedNone")}
        </div>
      )}

      {isManual && (
        <div className="flex flex-col gap-3 mt-4">
          <label className="block">
            <span className={FIELD_LABEL}>{t("settings.proxy.httpLabel")}</span>
            <input
              className={INPUT + " mt-1.5 w-full"}
              data-testid="proxy-http"
              value={draft.http}
              placeholder={t("settings.proxy.httpPlaceholder")}
              onChange={(e) => patch({ http: e.target.value })}
            />
          </label>
          <label className="block">
            <span className={FIELD_LABEL}>{t("settings.proxy.httpsLabel")}</span>
            <input
              className={INPUT + " mt-1.5 w-full"}
              data-testid="proxy-https"
              value={draft.https}
              placeholder={t("settings.proxy.httpsPlaceholder")}
              onChange={(e) => patch({ https: e.target.value })}
            />
          </label>
          <label className="block">
            <span className={FIELD_LABEL}>{t("settings.proxy.noProxyLabel")}</span>
            <input
              className={INPUT + " mt-1.5 w-full"}
              data-testid="proxy-no-proxy"
              value={draft.no_proxy}
              onChange={(e) => patch({ no_proxy: e.target.value })}
            />
            <span className={FIELD_HELP}>{t("settings.proxy.noProxyHelp")}</span>
          </label>
          <div className="flex items-center gap-3">
            <button className={BTN_ACCENT} data-testid="proxy-save" onClick={() => persist(draft)}>
              {t("settings.proxy.save")}
            </button>
            {isSaved && (
              <span className="text-[12px] text-muted">{t("settings.proxy.saved")}</span>
            )}
          </div>
        </div>
      )}

      {!isManual && (
        <div className={FIELD_HELP} data-testid="proxy-applies-next">
          {t("settings.proxy.appliesNext")}
        </div>
      )}
      {error && (
        <div className="text-[12px] text-danger mt-2" data-testid="proxy-error">
          {error}
        </div>
      )}
    </div>
  );
}

function SidebarCard() {
  const { t } = useI18n();
  const [peek, setPeek] = useState<number | null>(null);
  // A failed read must not show 5 as the saved count.
  const [hasFailed, setFailed] = useState(false);

  const load = () => {
    setFailed(false);
    getSettingsStrict()
      .then((s) => setPeek(s.sessions_peek || 5))
      .catch(() => setFailed(true));
  };
  useEffect(load, []);

  const save = async (n: number) => {
    const clamped = Math.max(1, Math.min(n || 5, 50));
    setPeek(clamped);
    await setSessionsPeek(clamped);
  };

  if (hasFailed && peek === null) return <LoadError what={t("state.what.settings", undefined, "these settings")} onRetry={load} />;
  if (peek === null) return null;
  return (
    <div className={CARD + " p-4 mb-4"}>
      <div className={FIELD_LABEL}>{t("settings.sidebar.title")}</div>
      <label className="flex items-center gap-3 mt-2.5">
        <span className="text-[13px] text-ink">{t("settings.sidebar.peekLabel")}</span>
        <input
          type="number"
          min={1}
          max={50}
          value={peek}
          className="w-16 px-2 py-1.5 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary"
          onChange={(e) => save(Number(e.target.value))}
        />
      </label>
      <div className={FIELD_HELP}>{t("settings.sidebar.peekHelp")}</div>
    </div>
  );
}

// -- Files (scratch location) — one card inside General (UX-021: a single option
// doesn't earn its own tab) -----------------------------------------------------
function FilesCard() {
  const { t } = useI18n();
  const [settings, setSettings] = useState<ModelSettings | null>(null);
  const [scratchDraft, setScratchDraft] = useState("");
  const [scratchMsg, setScratchMsg] = useState<string | null>(null);
  const desktop = isTauri();

  const [hasFailed, setFailed] = useState(false);

  const refresh = () => {
    setFailed(false);
    return getSettingsStrict()
      .then((s) => {
        setSettings(s);
        setScratchDraft((d) => d || s.scratch_base || "");
      })
      .catch(() => setFailed(true));
  };
  useEffect(() => {
    refresh();
  }, []);

  const saveScratch = async () => {
    setScratchMsg(null);
    const res = await setScratchBase(scratchDraft.trim());
    if (res.ok) {
      setScratchMsg(t("settings.files.savedMsg"));
      refresh();
    } else {
      setScratchMsg(res.error || t("settings.files.errorMsg"));
    }
  };
  const browseScratch = async () => {
    const picked = await pickFolder();
    if (picked) setScratchDraft(picked);
  };

  if (hasFailed && !settings) return <LoadError what={t("state.what.settings", undefined, "these settings")} onRetry={refresh} />;
  if (!settings) return null;

  return (
    <div className={CARD + " p-4 mb-4"}>
      <div className={FIELD_LABEL}>{t("settings.files.title")}</div>
        <div className="flex items-center gap-2 mt-2.5">
          <input
            className={INPUT}
            type="text"
            placeholder="~/delta"
            value={scratchDraft}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => setScratchDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && saveScratch()}
          />
          {desktop && (
            <button
              className="text-[12.5px] px-3 py-2 rounded-lg border border-primary bg-selected text-primary shrink-0 hover:opacity-80"
              onClick={browseScratch}
              title={t("settings.files.pickFolderTitle")}
            >
              {t("settings.files.browse")}
            </button>
          )}
          <button className={BTN_ACCENT} onClick={saveScratch} disabled={!scratchDraft.trim()}>
            {t("common.save")}
          </button>
        </div>
      <div className={FIELD_HELP}>{t("settings.files.help")}</div>
      {scratchMsg && <div className="text-[12.5px] text-muted mt-2.5">{scratchMsg}</div>}
    </div>
  );
}
