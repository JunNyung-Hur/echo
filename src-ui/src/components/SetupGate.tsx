import { useEffect, useState, useCallback, type ReactNode } from "react";
import { CheckCircle2, X } from "lucide-react";
import { toast } from "sonner";
import { openUrl } from "@tauri-apps/plugin-opener";

import { useLang, LANGS } from "@/i18n/LangContext";
import { type Lang } from "@/i18n/dict";
import { settingsApi } from "@/api/settings";
import { endpointsApi, type EndpointKind } from "@/api/endpoints";

type Phase = "checking" | "lang" | "setup" | "done" | "ready";

/**
 * First-run setup gate (phase-05). Blocks the main app until:
 *  1) a UI language is chosen (settings `ui_lang`), then
 *  2) the first setup is completed (settings `setup_completed`).
 *
 * After the first completion, models are managed in Settings — deleting them
 * never re-triggers the gate. ASR is optional (skippable); only an LLM is
 * needed to finish. Installer-style steps share the `echo` wordmark:
 *   Step 1 language → Step 2 LLM wizard → Step 3 ASR wizard (skippable) → done.
 */
export default function SetupGate({ children }: { children: ReactNode }) {
  const { setLang } = useLang();
  const [phase, setPhase] = useState<Phase>("checking");

  const hasLlmActive = useCallback(async () => {
    const llm = await endpointsApi.list("llm");
    return llm.some((e) => e.is_active === 1);
  }, []);

  const recheck = useCallback(async () => {
    try {
      const uiLang = await settingsApi.get("ui_lang");
      if (uiLang !== "ko" && uiLang !== "en") {
        setPhase("lang");
        return;
      }
      // 첫 셋업을 한 번 끝냈으면 이후엔 모델을 지워도 게이트를 다시 띄우지 않는다.
      // (모델 관리는 설정 화면에서 한다.)
      if ((await settingsApi.get("setup_completed")) === "1") {
        setPhase("ready");
        return;
      }
      // 플래그는 없지만 이미 활성 LLM이 있는 기존 사용자는 '완료'로 소급 처리.
      if (await hasLlmActive()) {
        await settingsApi.set("setup_completed", "1");
        setPhase("ready");
        return;
      }
      setPhase("setup");
    } catch {
      // 게이트 자체 오류로 앱을 막지 않는다 — 최악의 경우 통과시킨다.
      setPhase("ready");
    }
  }, [hasLlmActive]);

  useEffect(() => {
    void recheck();
  }, [recheck]);

  if (phase === "checking") return null;

  if (phase === "lang") {
    return (
      <LanguageScreen
        onNext={async (code) => {
          setLang(code);
          await settingsApi.set("ui_lang", code);
          setPhase("setup");
        }}
      />
    );
  }

  if (phase === "setup") {
    return <ModelSetupScreen onBack={() => setPhase("lang")} onDone={() => setPhase("done")} />;
  }

  if (phase === "done") {
    return <CompletionScreen onContinue={() => setPhase("ready")} />;
  }

  return <div className="app-enter">{children}</div>;
}

/** Brief "all set" interstitial after setup, then auto-advances to the app. */
function CompletionScreen({ onContinue }: { onContinue: () => void }) {
  const { lang } = useLang();
  const en = lang === "en";
  useEffect(() => {
    const t = setTimeout(onContinue, 2000);
    return () => clearTimeout(t);
  }, [onContinue]);
  return (
    <div className="min-h-screen flex flex-col items-center justify-center gap-4 bg-white setup-done">
      <CheckCircle2 className="w-14 h-14 text-emerald-500" strokeWidth={1.5} />
      <div className="text-xl font-semibold text-gray-900">
        {en ? "All set!" : "설정을 완료했어요"}
      </div>
    </div>
  );
}

/** Brand wordmark — Space Grotesk + sky→indigo gradient. */
function Wordmark() {
  return (
    <h1
      className="inline-block text-6xl font-medium tracking-tight bg-gradient-to-r from-sky-400 to-indigo-500 bg-clip-text text-transparent select-none"
      style={{ fontFamily: "'Space Grotesk', sans-serif" }}
    >
      echo
    </h1>
  );
}

/**
 * Full-screen installer shell (used by the setup gate). Wordmark top-left; an
 * outer region holds the Step header + Back/Next, with the step content
 * top-aligned in a lightly-tinted inner container.
 */
function SetupStep({
  step,
  desc,
  children,
  footer,
}: {
  step: string;
  desc: string;
  children: ReactNode;
  footer: ReactNode;
}) {
  return (
    <div className="min-h-screen flex flex-col bg-white px-12 py-10">
      <div>
        <Wordmark />
      </div>

      <div className="flex-1 flex items-start justify-center pt-16">
        <div className="w-full max-w-xl min-h-[440px] flex flex-col">
          <div>
            <div className="text-xl font-semibold text-gray-900">{step}</div>
            <p className="text-sm text-gray-500 mt-1">{desc}</p>
          </div>

          <div className="flex-1 flex items-start justify-center pt-12">
            <div className="w-full max-w-xl">
              <div className="rounded-2xl bg-slate-50 px-7 py-8">{children}</div>
              <div className="flex justify-between items-center mt-12">{footer}</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/** Modal shell (used by Settings "add model"). Title + close + content + footer. */
export function ModalFrame({
  title,
  footer,
  onClose,
  children,
}: {
  title: string;
  footer: ReactNode;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-6">
      <div className="w-full max-w-lg bg-white rounded-2xl shadow-xl flex flex-col max-h-[85vh]">
        <div className="flex items-center justify-between px-7 pt-6 pb-1">
          <h2 className="text-lg font-semibold text-gray-900">{title}</h2>
          <button
            type="button"
            onClick={onClose}
            className="p-1 text-gray-400 hover:text-gray-700 bg-transparent border-0 cursor-pointer"
          >
            <X className="w-5 h-5" />
          </button>
        </div>
        <div className="flex-1 overflow-auto px-7 py-5">
          <div className="rounded-2xl bg-slate-50 px-6 py-7">{children}</div>
        </div>
        <div className="flex justify-between items-center px-7 pb-6 pt-1">{footer}</div>
      </div>
    </div>
  );
}

/** Question above its body (choices / input). */
function CenteredAsk({
  question,
  hint,
  children,
}: {
  question: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div>
      <h2 className="text-base font-medium text-gray-900">{question}</h2>
      {hint && <div className="text-xs text-gray-400 mt-1.5">{hint}</div>}
      <div className="mt-5">{children}</div>
    </div>
  );
}

const primaryBtn =
  "px-7 py-2.5 bg-sky-600 text-white rounded-lg hover:bg-sky-700 transition-colors text-sm font-medium cursor-pointer border-0 shadow-sm disabled:opacity-40 disabled:cursor-not-allowed";
const ghostBtn =
  "py-2.5 pr-5 text-gray-500 hover:text-gray-900 text-sm bg-transparent border-0 cursor-pointer";
const skipBtn =
  "py-2.5 text-sm text-gray-500 hover:text-gray-900 bg-transparent border-0 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed";
const testBtn =
  "px-6 py-2.5 text-sm font-medium rounded-lg border border-sky-200 text-sky-700 bg-white hover:bg-sky-50 hover:border-sky-300 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed shrink-0";

/** Step 1 — installer-style language picker. */
function LanguageScreen({ onNext }: { onNext: (code: Lang) => void }) {
  const [selected, setSelected] = useState<Lang>(LANGS[0].code);
  return (
    <SetupStep
      step="Step 1"
      desc="언어를 선택해 주세요 · Choose your language"
      footer={
        <>
          <span />
          <button type="button" onClick={() => onNext(selected)} className={primaryBtn}>
            Next
          </button>
        </>
      }
    >
      <CenteredAsk question="언어 · Language">
        <div className="w-full border border-gray-300 rounded-lg overflow-hidden divide-y divide-gray-100">
          {LANGS.map((l) => (
            <button
              key={l.code}
              type="button"
              onClick={() => setSelected(l.code)}
              onDoubleClick={() => onNext(l.code)}
              className={`block w-full text-left px-4 py-2.5 text-sm border-0 cursor-pointer transition-colors ${
                selected === l.code
                  ? "bg-sky-500 text-white"
                  : "bg-white text-gray-800 hover:bg-sky-50"
              }`}
            >
              {l.label}
            </button>
          ))}
        </div>
      </CenteredAsk>
    </SetupStep>
  );
}

export interface EpForm {
  name: string;
  model_id: string;
  api_base_url: string;
  api_key: string;
  request_mode: string;
}

const OPENAI_BASE = "https://api.openai.com/v1";

// NOTE: OpenAI 모델 목록은 수동 관리 — 새 모델 출시/가격 변동 시 주기적으로 갱신 필요.
const OPENAI_LLM_MODELS = [
  {
    id: "gpt-5.4-nano-2026-03-17",
    title: "GPT-5.4 nano",
    ko: "가장 저렴하고 빨라요",
    en: "Cheapest and fastest",
  },
  {
    id: "gpt-5.4-mini-2026-03-17",
    title: "GPT-5.4 mini",
    ko: "저렴하고 충분히 똑똑해요 (추천)",
    en: "Affordable and capable (recommended)",
  },
  {
    id: "gpt-5.5-2026-04-23",
    title: "GPT-5.5",
    ko: "가장 똑똑하지만 매우 비싸요",
    en: "Smartest, but very pricey",
  },
];
const OPENAI_ASR_MODELS = [
  {
    id: "gpt-4o-mini-transcribe-2025-12-15",
    title: "GPT-4o mini Transcribe",
    ko: "저렴하고 성능도 준수해요 (추천)",
    en: "Affordable and solid (recommended)",
  },
  {
    id: "gpt-4o-transcribe",
    title: "GPT-4o Transcribe",
    ko: "최고 성능, 더 비싸요",
    en: "Top quality, pricier",
  },
];

const oaKeyHint = (en: boolean) => (
  <>
    {en && "Create one at "}
    <button
      type="button"
      onClick={() => openUrl("https://platform.openai.com/api-keys")}
      className="text-sky-600 hover:underline bg-transparent border-0 p-0 cursor-pointer"
    >
      platform.openai.com/api-keys
    </button>
    {en ? "." : " 에서 발급할 수 있어요."}
  </>
);

type RenderFrame = (parts: { body: ReactNode; footer: ReactNode }) => ReactNode;

/**
 * Steps 2 & 3 of the gate — the LLM wizard (required) then the ASR wizard
 * (optional/skippable). Finishing registers + activates what was set up and
 * stamps `setup_completed`.
 */
function ModelSetupScreen({ onBack, onDone }: { onBack: () => void; onDone: () => void }) {
  const { lang } = useLang();
  const en = lang === "en";
  const [sub, setSub] = useState<"llm" | "asr">("llm");
  const [llmForm, setLlmForm] = useState<EpForm | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const setupFrame =
    (step: string, desc: string): RenderFrame =>
    ({ body, footer }) => (
      <SetupStep step={step} desc={desc} footer={footer}>
        {body}
      </SetupStep>
    );

  if (sub === "llm") {
    return (
      <LlmWizard
        finishLabel="next"
        renderFrame={setupFrame(
          "Step 2",
          en
            ? "Connect the AI model your echo agent will use."
            : "echo 에이전트가 사용할 AI 모델을 연결하는 단계에요.",
        )}
        onBack={onBack}
        onComplete={(f) => {
          setLlmForm(f);
          setSub("asr");
        }}
      />
    );
  }

  // asrForm null = ASR 건너뛰기 (LLM만 등록).
  async function finish(asrForm: EpForm | null) {
    if (!llmForm || submitting) return;
    setSubmitting(true);
    try {
      const l = await endpointsApi.create({ kind: "llm", ...llmForm });
      await endpointsApi.activate(l.id);
      if (asrForm) {
        const a = await endpointsApi.create({ kind: "asr", ...asrForm });
        await endpointsApi.activate(a.id);
      }
      await settingsApi.set("setup_completed", "1");
      onDone();
    } catch (e) {
      toast.error(String(e));
      setSubmitting(false);
    }
  }

  return (
    <AsrWizard
      finishLabel="done"
      submitting={submitting}
      showSkip
      onSkip={() => finish(null)}
      renderFrame={setupFrame(
        "Step 3",
        en
          ? "Connect a speech-to-text model. (Optional — recording is unavailable without it)"
          : "음성 인식 모델을 연결하는 단계에요. (선택 · 미설정 시 녹음 기능을 사용할 수 없어요)",
      )}
      onBack={() => setSub("llm")}
      onComplete={(f) => finish(f)}
    />
  );
}

/**
 * Add a single endpoint from Settings, reusing the wizards inside a modal.
 * Registers (and activates if it's the first of its kind) on finish.
 */
export function AddModelModal({
  kind,
  onClose,
  onAdded,
}: {
  kind: EndpointKind;
  onClose: () => void;
  onAdded: () => void;
}) {
  const { lang } = useLang();
  const en = lang === "en";
  const [submitting, setSubmitting] = useState(false);

  async function register(form: EpForm) {
    if (submitting) return;
    setSubmitting(true);
    try {
      const ep = await endpointsApi.create({ kind, ...form });
      const list = await endpointsApi.list(kind);
      if (!list.some((e) => e.is_active === 1)) await endpointsApi.activate(ep.id);
      onAdded();
      onClose();
    } catch (e) {
      toast.error(String(e));
      setSubmitting(false);
    }
  }

  const frame: RenderFrame = ({ body, footer }) => (
    <ModalFrame
      title={
        kind === "llm"
          ? en
            ? "Add an AI model"
            : "AI 모델 추가"
          : en
            ? "Add a speech-to-text model"
            : "음성 인식 모델 추가"
      }
      onClose={onClose}
      footer={footer}
    >
      {body}
    </ModalFrame>
  );

  return kind === "llm" ? (
    <LlmWizard finishLabel="add" submitting={submitting} renderFrame={frame} onBack={onClose} onComplete={register} />
  ) : (
    <AsrWizard
      finishLabel="add"
      submitting={submitting}
      renderFrame={frame}
      onBack={onClose}
      onComplete={(f) => f && register(f)}
    />
  );
}

type WizStep = "provider" | "oa-key" | "oa-model" | "oa-model-custom" | "cu-url" | "cu-key" | "cu-model" | "cu-alias" | "confirm";

/**
 * Interactive LLM wizard. `renderFrame` decides the shell (full-screen step or
 * modal); `finishLabel` decides the confirm button ("next" in the gate, "add"
 * in Settings). Choices advance on click; inputs advance on Enter.
 */
function LlmWizard({
  renderFrame,
  onBack,
  onComplete,
  finishLabel,
  submitting,
}: {
  renderFrame: RenderFrame;
  onBack: () => void;
  onComplete: (f: EpForm) => void;
  finishLabel: "next" | "add";
  submitting?: boolean;
}) {
  const { lang } = useLang();
  const en = lang === "en";
  const tx = (ko: string, e: string) => (en ? e : ko);
  const [hist, setHist] = useState<WizStep[]>([]);
  const [step, setStep] = useState<WizStep>("provider");
  const [provider, setProvider] = useState<"openai" | "custom" | null>(null);
  const [oaKey, setOaKey] = useState("");
  const [oaModel, setOaModel] = useState(OPENAI_LLM_MODELS[0].id);
  const [cuUrl, setCuUrl] = useState("");
  const [cuKey, setCuKey] = useState("");
  const [cuModel, setCuModel] = useState("");
  const [cuAlias, setCuAlias] = useState("");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);

  const go = (next: WizStep) => {
    setHist((h) => [...h, step]);
    setStep(next);
  };
  const back = () => {
    setTestResult(null);
    setHist((h) => {
      if (h.length === 0) {
        onBack();
        return h;
      }
      setStep(h[h.length - 1]);
      return h.slice(0, -1);
    });
  };

  const built = (): EpForm =>
    provider === "openai"
      ? {
          name: `OpenAI · ${OPENAI_LLM_MODELS.find((m) => m.id === oaModel)?.title ?? oaModel}`,
          model_id: oaModel,
          api_base_url: OPENAI_BASE,
          api_key: oaKey,
          request_mode: "chat_completions",
        }
      : {
          name: cuAlias.trim() || cuModel.trim(),
          model_id: cuModel.trim(),
          api_base_url: cuUrl.trim(),
          api_key: cuKey,
          request_mode: "chat_completions",
        };

  async function runTest() {
    const f = built();
    if (!f.model_id || !f.api_base_url) return;
    setTesting(true);
    setTestResult(null);
    try {
      const ep = await endpointsApi.create({
        kind: "llm",
        name: "__setup_test__",
        model_id: f.model_id,
        api_base_url: f.api_base_url,
        api_key: f.api_key,
        request_mode: "chat_completions",
      });
      try {
        const r = await endpointsApi.test(ep.id);
        setTestResult({ ok: r.success, msg: r.message });
      } finally {
        await endpointsApi.delete(ep.id);
      }
    } catch (e) {
      setTestResult({ ok: false, msg: String(e) });
    } finally {
      setTesting(false);
    }
  }

  const nextAction: { onNext: () => void; canNext: boolean } | null = (() => {
    switch (step) {
      case "oa-key":
        return { onNext: () => go("oa-model"), canNext: !!oaKey.trim() };
      case "oa-model-custom":
        return { onNext: () => go("confirm"), canNext: !!oaModel.trim() };
      case "cu-url":
        return { onNext: () => go("cu-key"), canNext: !!cuUrl.trim() };
      case "cu-key":
        return { onNext: () => go("cu-model"), canNext: true };
      case "cu-model":
        return { onNext: () => go("cu-alias"), canNext: !!cuModel.trim() };
      case "cu-alias":
        return { onNext: () => go("confirm"), canNext: true };
      default:
        return null;
    }
  })();

  const finishText = finishLabel === "add" ? tx("추가", "Add") : tx("다음", "Next");

  const footer = (
    <>
      <button type="button" onClick={back} className={ghostBtn}>
        {tx("뒤로", "Back")}
      </button>
      {step === "confirm" ? (
        <div className="flex items-center gap-3">
          {testResult && (
            <span className={`text-xs ${testResult.ok ? "text-emerald-600" : "text-red-600"}`}>
              {testResult.msg}
            </span>
          )}
          <button type="button" onClick={runTest} disabled={testing || !!submitting} className={testBtn}>
            {testing ? tx("테스트 중…", "Testing…") : tx("테스트하기", "Test")}
          </button>
          <button
            type="button"
            onClick={() => onComplete(built())}
            disabled={!!submitting}
            className={primaryBtn}
          >
            {submitting ? tx("저장 중…", "Saving…") : finishText}
          </button>
        </div>
      ) : nextAction ? (
        <button
          type="button"
          onClick={nextAction.onNext}
          disabled={!nextAction.canNext || !!submitting}
          className={primaryBtn}
        >
          {tx("다음", "Next")}
        </button>
      ) : (
        <span />
      )}
    </>
  );

  const body = (
    <div key={step} className="wizard-step">
      {step === "provider" && (
        <CenteredAsk question={tx("어떤 AI를 사용하시나요?", "Which AI will you use?")}>
          <div className="space-y-2.5">
            <ChoiceCard
              title="OpenAI"
              sub={tx("OpenAI가 제공하는 모델을 사용할게요 (API Key가 필요합니다)", "Use models provided by OpenAI (API key required)")}
              onClick={() => {
                setProvider("openai");
                go("oa-key");
              }}
            />
            <ChoiceCard
              title={tx("OpenAI API 호환 모델", "OpenAI-compatible model")}
              sub={tx("직접 서빙하는 모델이 있어요 (OpenAI Endpoint를 호환하는 모델)", "I have my own model (compatible with the OpenAI endpoint)")}
              onClick={() => {
                setProvider("custom");
                go("cu-url");
              }}
            />
          </div>
        </CenteredAsk>
      )}

      {step === "oa-key" && (
        <WizardInput
          question={tx("API 키를 입력해 주세요.", "Enter your OpenAI API Key.")}
          hint={oaKeyHint(en)}
          value={oaKey}
          onChange={setOaKey}
          type="password"
          placeholder="sk-…"
          canNext={!!oaKey.trim()}
          onNext={() => go("oa-model")}
        />
      )}

      {step === "oa-model" && (
        <CenteredAsk question={tx("어떤 모델을 사용하시고 싶으세요?", "Which model?")}>
          <div className="space-y-2.5">
            {OPENAI_LLM_MODELS.map((m) => (
              <ChoiceCard
                key={m.id}
                title={m.title}
                sub={tx(m.ko, m.en)}
                badge={m.id}
                onClick={() => {
                  setOaModel(m.id);
                  go("confirm");
                }}
              />
            ))}
            <ChoiceCard
              title={tx("직접 입력", "Custom")}
              sub={tx("모델 이름을 직접 입력해요", "Enter the model name yourself")}
              onClick={() => {
                setOaModel("");
                go("oa-model-custom");
              }}
            />
          </div>
        </CenteredAsk>
      )}

      {step === "oa-model-custom" && (
        <WizardInput
          question={tx("모델 이름을 입력해 주세요.", "Enter the model name.")}
          hint={tx("OpenAI 모델 ID예요. 예: gpt-5.4-nano-2026-03-17", "OpenAI model ID. e.g. gpt-5.4-nano-2026-03-17")}
          value={oaModel}
          onChange={setOaModel}
          placeholder="gpt-5.4-nano-2026-03-17"
          canNext={!!oaModel.trim()}
          onNext={() => go("confirm")}
        />
      )}

      {step === "cu-url" && (
        <WizardInput
          question={tx("API 주소를 입력해 주세요.", "Enter the API base URL.")}
          hint={tx("/v1 까지 입력해 주세요. 예: http://localhost:11434/v1", "Include up to /v1. e.g. http://localhost:11434/v1")}
          value={cuUrl}
          onChange={setCuUrl}
          placeholder="http://localhost:11434/v1"
          canNext={!!cuUrl.trim()}
          onNext={() => go("cu-key")}
        />
      )}

      {step === "cu-key" && (
        <WizardInput
          question={tx("API 키를 입력해 주세요.", "Enter the API key.")}
          hint={tx("Key가 필요 없으면 비워 두셔도 괜찮아요.", "Leave it blank if no key is needed.")}
          value={cuKey}
          onChange={setCuKey}
          type="password"
          placeholder="sk-…"
          canNext
          onNext={() => go("cu-model")}
        />
      )}

      {step === "cu-model" && (
        <WizardInput
          question={tx("모델 이름을 입력해 주세요.", "Enter the model name.")}
          hint={tx("예: google/gemma-4-31B-it, Qwen/Qwen3.6-35B-A3B", "e.g. google/gemma-4-31B-it, Qwen/Qwen3.6-35B-A3B")}
          value={cuModel}
          onChange={setCuModel}
          placeholder="google/gemma-4-31B-it"
          canNext={!!cuModel.trim()}
          onNext={() => go("cu-alias")}
        />
      )}

      {step === "cu-alias" && (
        <WizardInput
          question={tx("이 모델을 부를 별명이 있을까요?", "A nickname for this model?")}
          hint={tx("설정 화면에서 쓰여요. 비우면 모델 이름을 사용해요.", "Shown in Settings. Defaults to the model name.")}
          value={cuAlias}
          onChange={setCuAlias}
          placeholder={cuModel || (en ? "My LLM" : "내 모델")}
          canNext
          onNext={() => go("confirm")}
        />
      )}

      {step === "confirm" && (
        <CenteredAsk
          question={tx(
            "AI 모델 설정이 끝났어요! 아래 정보가 맞는지 확인해 주세요.",
            "Your AI model is ready — please double-check the details below.",
          )}
        >
          <ModelCard form={built()} en={en} />
        </CenteredAsk>
      )}
    </div>
  );

  return renderFrame({ body, footer });
}

/**
 * Interactive ASR wizard. Same shape as the LLM wizard, plus a request-mode
 * step and an optional Skip button (gate only) on the provider screen.
 */
function AsrWizard({
  renderFrame,
  onBack,
  onComplete,
  finishLabel,
  submitting,
  showSkip,
  onSkip,
}: {
  renderFrame: RenderFrame;
  onBack: () => void;
  onComplete: (f: EpForm | null) => void;
  finishLabel: "done" | "add";
  submitting?: boolean;
  showSkip?: boolean;
  onSkip?: () => void;
}) {
  const { lang } = useLang();
  const en = lang === "en";
  const tx = (ko: string, e: string) => (en ? e : ko);
  const [hist, setHist] = useState<WizStep[]>([]);
  const [step, setStep] = useState<WizStep>("provider");
  const [provider, setProvider] = useState<"openai" | "custom" | null>(null);
  const [oaKey, setOaKey] = useState("");
  const [oaModel, setOaModel] = useState(OPENAI_ASR_MODELS[0].id);
  const [cuUrl, setCuUrl] = useState("");
  const [cuKey, setCuKey] = useState("");
  const [cuModel, setCuModel] = useState("");
  const [cuMode, setCuMode] = useState("transcriptions");
  const [cuAlias, setCuAlias] = useState("");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);

  const go = (next: WizStep) => {
    setHist((h) => [...h, step]);
    setStep(next);
  };
  const back = () => {
    setTestResult(null);
    setHist((h) => {
      if (h.length === 0) {
        onBack();
        return h;
      }
      setStep(h[h.length - 1]);
      return h.slice(0, -1);
    });
  };

  const built = (): EpForm =>
    provider === "openai"
      ? {
          name: `OpenAI · ${OPENAI_ASR_MODELS.find((m) => m.id === oaModel)?.title ?? oaModel}`,
          model_id: oaModel,
          api_base_url: OPENAI_BASE,
          api_key: oaKey,
          request_mode: "transcriptions",
        }
      : {
          name: cuAlias.trim() || cuModel.trim(),
          model_id: cuModel.trim(),
          api_base_url: cuUrl.trim(),
          api_key: cuKey,
          request_mode: cuMode,
        };

  async function runTest() {
    const f = built();
    if (!f.model_id || !f.api_base_url) return;
    setTesting(true);
    setTestResult(null);
    try {
      const ep = await endpointsApi.create({
        kind: "asr",
        name: "__setup_test__",
        model_id: f.model_id,
        api_base_url: f.api_base_url,
        api_key: f.api_key,
        request_mode: f.request_mode,
      });
      try {
        const r = await endpointsApi.test(ep.id);
        setTestResult({ ok: r.success, msg: r.message });
      } finally {
        await endpointsApi.delete(ep.id);
      }
    } catch (e) {
      setTestResult({ ok: false, msg: String(e) });
    } finally {
      setTesting(false);
    }
  }

  const nextAction: { onNext: () => void; canNext: boolean } | null = (() => {
    switch (step) {
      case "oa-key":
        return { onNext: () => go("oa-model"), canNext: !!oaKey.trim() };
      case "oa-model-custom":
        return { onNext: () => go("confirm"), canNext: !!oaModel.trim() };
      case "cu-url":
        return { onNext: () => go("cu-key"), canNext: !!cuUrl.trim() };
      case "cu-key":
        return { onNext: () => go("cu-model"), canNext: true };
      case "cu-model":
        return { onNext: () => go("cu-alias"), canNext: !!cuModel.trim() };
      case "cu-alias":
        return { onNext: () => go("confirm"), canNext: true };
      default:
        return null;
    }
  })();

  const finishText = finishLabel === "add" ? tx("추가", "Add") : tx("완료", "Done");

  const footer = (
    <>
      <button type="button" onClick={back} className={ghostBtn}>
        {tx("뒤로", "Back")}
      </button>
      {step === "provider" ? (
        showSkip ? (
          <button type="button" onClick={onSkip} disabled={!!submitting} className={skipBtn}>
            {submitting ? tx("저장 중…", "Saving…") : tx("건너뛰기", "Skip")}
          </button>
        ) : (
          <span />
        )
      ) : step === "confirm" ? (
        <div className="flex items-center gap-3">
          {testResult && (
            <span className={`text-xs ${testResult.ok ? "text-emerald-600" : "text-red-600"}`}>
              {testResult.msg}
            </span>
          )}
          <button type="button" onClick={runTest} disabled={testing || !!submitting} className={testBtn}>
            {testing ? tx("테스트 중…", "Testing…") : tx("테스트하기", "Test")}
          </button>
          <button
            type="button"
            onClick={() => onComplete(built())}
            disabled={!!submitting}
            className={primaryBtn}
          >
            {submitting ? tx("저장 중…", "Saving…") : finishText}
          </button>
        </div>
      ) : nextAction ? (
        <button
          type="button"
          onClick={nextAction.onNext}
          disabled={!nextAction.canNext || !!submitting}
          className={primaryBtn}
        >
          {tx("다음", "Next")}
        </button>
      ) : (
        <span />
      )}
    </>
  );

  const body = (
    <div key={step} className="wizard-step">
      {step === "provider" && (
        <CenteredAsk question={tx("어떤 AI를 사용하시나요?", "Which AI will you use?")}>
          <div className="space-y-2.5">
            <ChoiceCard
              title="OpenAI"
              sub={tx("OpenAI가 제공하는 모델을 사용할게요 (API Key가 필요합니다)", "Use models provided by OpenAI (API key required)")}
              onClick={() => {
                setProvider("openai");
                go("oa-key");
              }}
            />
            <ChoiceCard
              title={tx("OpenAI API 호환 모델", "OpenAI-compatible model")}
              sub={tx("직접 서빙하는 모델이 있어요 (OpenAI Endpoint를 호환하는 모델)", "I have my own model (compatible with the OpenAI endpoint)")}
              onClick={() => {
                setProvider("custom");
                go("cu-url");
              }}
            />
          </div>
        </CenteredAsk>
      )}

      {step === "oa-key" && (
        <WizardInput
          question={tx("OpenAI API Key를 입력해 주세요.", "Enter your OpenAI API Key.")}
          hint={oaKeyHint(en)}
          value={oaKey}
          onChange={setOaKey}
          type="password"
          placeholder="sk-…"
          canNext={!!oaKey.trim()}
          onNext={() => go("oa-model")}
        />
      )}

      {step === "oa-model" && (
        <CenteredAsk question={tx("어떤 모델을 사용할까요?", "Which model?")}>
          <div className="space-y-2.5">
            {OPENAI_ASR_MODELS.map((m) => (
              <ChoiceCard
                key={m.id}
                title={m.title}
                sub={tx(m.ko, m.en)}
                badge={m.id}
                onClick={() => {
                  setOaModel(m.id);
                  go("confirm");
                }}
              />
            ))}
            <ChoiceCard
              title={tx("직접 입력", "Custom")}
              sub={tx("모델 이름을 직접 입력해요", "Enter the model name yourself")}
              onClick={() => {
                setOaModel("");
                go("oa-model-custom");
              }}
            />
          </div>
        </CenteredAsk>
      )}

      {step === "oa-model-custom" && (
        <WizardInput
          question={tx("모델 이름을 입력해 주세요.", "Enter the model name.")}
          hint={tx("OpenAI 모델 ID예요. 예: gpt-4o-transcribe", "OpenAI model ID. e.g. gpt-4o-transcribe")}
          value={oaModel}
          onChange={setOaModel}
          placeholder="gpt-4o-transcribe"
          canNext={!!oaModel.trim()}
          onNext={() => go("confirm")}
        />
      )}

      {step === "cu-url" && (
        <WizardInput
          question={tx("준비하신 모델의 API 주소를 입력해 주세요.", "Enter your model's API base URL.")}
          hint={tx("/v1 까지 입력해 주세요. 예: http://localhost:11434/v1", "Include up to /v1. e.g. http://localhost:11434/v1")}
          value={cuUrl}
          onChange={setCuUrl}
          placeholder="http://localhost:11434/v1"
          canNext={!!cuUrl.trim()}
          onNext={() => go("cu-key")}
        />
      )}

      {step === "cu-key" && (
        <WizardInput
          question={tx("API Key를 입력해 주세요.", "Enter the API key.")}
          hint={tx("Key가 필요 없으면 비워 두셔도 괜찮아요.", "Leave it blank if no key is needed.")}
          value={cuKey}
          onChange={setCuKey}
          type="password"
          placeholder="sk-…"
          canNext
          onNext={() => go("cu-model")}
        />
      )}

      {step === "cu-model" && (
        <CenteredAsk
          question={tx("음성 인식 모델의 이름과 호출 방식을 알려 주세요.", "Tell me the model name and how to call it.")}
        >
          <div className="space-y-4">
            <div>
              <span className="text-[11px] text-gray-500 mb-1 block">{tx("모델 이름", "Model name")}</span>
              <input
                autoFocus
                value={cuModel}
                onChange={(e) => setCuModel(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && cuModel.trim()) go("cu-alias");
                }}
                placeholder="예: openai/whisper-large-v3, Qwen3-ASR"
                className="form-input"
              />
            </div>
            <div>
              <span className="text-[11px] text-gray-500 mb-1 block">{tx("호출 방식", "Request mode")}</span>
              <select
                value={cuMode}
                onChange={(e) => setCuMode(e.target.value)}
                className="form-input bg-white"
              >
                <option value="transcriptions">Transcriptions (v1/audio)</option>
                <option value="chat_completions">Completions (v1/chat)</option>
              </select>
            </div>
          </div>
        </CenteredAsk>
      )}

      {step === "cu-alias" && (
        <WizardInput
          question={tx("이 모델을 부를 별명이 있나요?", "A nickname for this model?")}
          hint={tx("설정 화면에서 쓰여요. 비우면 모델 이름을 사용해요.", "Shown in Settings. Defaults to the model name.")}
          value={cuAlias}
          onChange={setCuAlias}
          placeholder={cuModel || (en ? "My ASR" : "내 음성 모델")}
          canNext
          onNext={() => go("confirm")}
        />
      )}

      {step === "confirm" && (
        <CenteredAsk
          question={tx(
            "음성 인식 모델 설정이 끝났어요! 아래 정보가 맞는지 확인해 주세요.",
            "Your ASR model is ready — please double-check the details below.",
          )}
        >
          <ModelCard form={built()} en={en} />
        </CenteredAsk>
      )}
    </div>
  );

  return renderFrame({ body, footer });
}

function ChoiceCard({
  title,
  sub,
  badge,
  onClick,
}: {
  title: string;
  sub?: string;
  badge?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="w-full text-left px-4 py-3.5 rounded-xl border border-gray-200 bg-white hover:border-sky-300 hover:bg-sky-50/40 transition-colors cursor-pointer"
    >
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-gray-900">{title}</div>
          {sub && <div className="text-xs text-gray-500 mt-0.5">{sub}</div>}
        </div>
        {badge && <code className="text-[10px] text-gray-400 shrink-0">{badge}</code>}
      </div>
    </button>
  );
}

function WizardInput({
  question,
  hint,
  value,
  onChange,
  onNext,
  canNext,
  type,
  placeholder,
}: {
  question: string;
  hint?: ReactNode;
  value: string;
  onChange: (v: string) => void;
  onNext: () => void;
  canNext: boolean;
  type?: string;
  placeholder?: string;
}) {
  return (
    <CenteredAsk question={question} hint={hint}>
      <input
        autoFocus
        type={type ?? "text"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && canNext) onNext();
        }}
        placeholder={placeholder}
        className="form-input"
      />
    </CenteredAsk>
  );
}

function ModelCard({ form, en }: { form: EpForm; en: boolean }) {
  const rows: [string, string][] = [
    [en ? "Name" : "이름", form.name],
    [en ? "URL" : "주소", form.api_base_url],
    [en ? "Model" : "모델", form.model_id],
    [en ? "Mode" : "방식", form.request_mode === "transcriptions" ? "Transcriptions" : "Completions"],
    ["API Key", form.api_key ? "••••••••" : en ? "(none)" : "(없음)"],
  ];
  return (
    <div className="rounded-xl border border-sky-200 bg-sky-50/40 p-5">
      <dl className="space-y-2.5">
        {rows.map(([k, v]) => (
          <div key={k} className="flex gap-3 text-sm">
            <dt className="w-16 shrink-0 text-gray-400">{k}</dt>
            <dd className="text-gray-900 break-all">{v || "—"}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
