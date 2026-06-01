import { useState, useEffect, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import {
  ArrowLeft,
  Plus,
  Pencil,
  Trash2,
  Zap,
  Loader2,
  Check,
  CheckCircle2,
  XCircle,
} from "lucide-react";
import { toast } from "sonner";

import {
  endpointsApi,
  AiEndpoint,
  EndpointKind,
  TestResult,
} from "@/api/endpoints";
import SourceSelector from "@/components/SourceSelector";
import { AddModelModal } from "@/components/SetupGate";
import { settingsApi } from "@/api/settings";
import { useLang, useT, LANGS } from "@/i18n/LangContext";

interface FormState {
  name: string;
  model_id: string;
  api_base_url: string;
  api_key: string;
  request_mode: string;
  chunk_seconds: string;
  max_tokens: string;
}

const emptyForm: FormState = {
  name: "",
  model_id: "",
  api_base_url: "",
  api_key: "",
  request_mode: "chat_completions",
  chunk_seconds: "",
  max_tokens: "",
};

// 설정 → 언어 탭. ui_lang 토글(LangContext = settings DB + localStorage 미러).
function LanguageSection() {
  const { lang, setLang, t } = useLang();
  return (
    <div className="w-full">
      <h2 className="text-sm font-medium text-gray-900 mb-3">{t("settings.lang.title")}</h2>
      <p className="text-xs text-gray-500 mb-4">{t("settings.lang.desc")}</p>
      <div className="space-y-2">
        {LANGS.map((l) => {
          const active = lang === l.code;
          return (
            <button
              key={l.code}
              onClick={() => setLang(l.code)}
              className={`w-full flex items-center justify-between px-4 py-2.5 rounded-full border text-sm font-medium cursor-pointer transition-colors ${
                active
                  ? "bg-sky-50 border-sky-300 text-sky-700"
                  : "bg-white border-gray-200 text-gray-700 hover:border-gray-300"
              }`}
            >
              <span>{l.label}</span>
              {active && <Check className="w-4 h-4 shrink-0" />}
            </button>
          );
        })}
      </div>
    </div>
  );
}

// 설정 → 알림 탭. 종류별 완료 알림 on/off (settings KV: notify_*). 기본 on.
function NotificationSection() {
  const { lang } = useLang();
  const en = lang === "en";
  const items = [
    {
      key: "notify_transcribe",
      label: en ? "Transcription complete" : "전사 완료",
      desc: en ? "When a recording finishes transcribing." : "녹음 전사가 끝났을 때 알려드려요.",
    },
    {
      key: "notify_note",
      label: en ? "Note ready" : "노트 준비 완료",
      desc: en ? "When the note has been organized." : "노트 정리가 끝났을 때 알려드려요.",
    },
  ];
  const [on, setOn] = useState<Record<string, boolean>>({ notify_transcribe: true, notify_note: true });
  useEffect(() => {
    void Promise.all([settingsApi.get("notify_transcribe"), settingsApi.get("notify_note")]).then(
      ([tr, nt]) => setOn({ notify_transcribe: tr !== "0", notify_note: nt !== "0" }),
    );
  }, []);
  const toggle = async (key: string) => {
    const next = !on[key];
    setOn((p) => ({ ...p, [key]: next }));
    try {
      await settingsApi.set(key, next ? "1" : "0");
    } catch (e) {
      toast.error(String(e));
    }
  };
  return (
    <div className="w-full">
      <h2 className="text-sm font-medium text-gray-900 mb-3">{en ? "Notifications" : "알림"}</h2>
      <p className="text-xs text-gray-500 mb-4">
        {en ? "Choose which completion alerts you want to receive." : "받을 완료 알림을 선택하세요."}
      </p>
      <div className="space-y-2">
        {items.map((it) => (
          <div
            key={it.key}
            className="flex items-center justify-between px-4 py-3 rounded-xl border border-gray-100 bg-white"
          >
            <div className="min-w-0">
              <div className="text-sm text-gray-800">{it.label}</div>
              <div className="text-xs text-gray-400 mt-0.5">{it.desc}</div>
            </div>
            <Toggle checked={on[it.key]} onChange={() => toggle(it.key)} />
          </div>
        ))}
      </div>
    </div>
  );
}

function Toggle({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={onChange}
      className={`relative w-10 h-6 rounded-full border-0 cursor-pointer transition-colors shrink-0 ${
        checked ? "bg-sky-500" : "bg-gray-300"
      }`}
    >
      <span
        className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm transition-transform ${
          checked ? "translate-x-4" : ""
        }`}
      />
    </button>
  );
}

export default function SettingsPage() {
  const navigate = useNavigate();
  const t = useT();
  const { lang } = useLang();
  const [section, setSection] = useState<"audio" | "models" | "language" | "notifications">("audio");
  const [tab, setTab] = useState<EndpointKind>("llm");
  const [endpoints, setEndpoints] = useState<AiEndpoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>(emptyForm);
  const [testResult, setTestResult] = useState<{ id: string; result: TestResult } | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [addOpen, setAddOpen] = useState(false);

  const fetch = useCallback(async () => {
    setLoading(true);
    try {
      setEndpoints(await endpointsApi.list(tab));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLoading(false);
    }
  }, [tab]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  const resetForm = () => {
    setShowForm(false);
    setEditingId(null);
    setForm(emptyForm);
  };

  const onSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const common = {
      name: form.name.trim(),
      model_id: form.model_id.trim(),
      api_base_url: form.api_base_url.trim(),
      api_key: form.api_key,
      request_mode: form.request_mode,
      chunk_seconds: form.chunk_seconds ? Number(form.chunk_seconds) : null,
      max_tokens: form.max_tokens ? Number(form.max_tokens) : null,
    };
    try {
      if (editingId) {
        await endpointsApi.update(editingId, common);
      } else {
        await endpointsApi.create({ kind: tab, ...common });
      }
      resetForm();
      fetch();
      toast.success(editingId ? t("settings.toast.updated") : t("settings.toast.added"));
    } catch (err) {
      toast.error(String(err));
    }
  };

  const onEdit = (ep: AiEndpoint) => {
    setForm({
      name: ep.name,
      model_id: ep.model_id,
      api_base_url: ep.api_base_url,
      api_key: ep.api_key,
      request_mode: ep.request_mode || "chat_completions",
      chunk_seconds: ep.chunk_seconds != null ? String(ep.chunk_seconds) : "",
      max_tokens: ep.max_tokens != null ? String(ep.max_tokens) : "",
    });
    setEditingId(ep.id);
    setShowForm(true);
  };

  const onDelete = async (id: string) => {
    if (!confirm(t("settings.delete.confirm"))) return;
    try {
      await endpointsApi.delete(id);
      fetch();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const onActivate = async (id: string) => {
    try {
      await endpointsApi.activate(id);
      fetch();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const onTest = async (ep: AiEndpoint) => {
    setTestingId(ep.id);
    setTestResult(null);
    try {
      const result = await endpointsApi.test(ep.id);
      setTestResult({ id: ep.id, result });
    } catch (e) {
      setTestResult({
        id: ep.id,
        result: { success: false, message: String(e), response_time_ms: null },
      });
    } finally {
      setTestingId(null);
    }
  };

  return (
    <div className="min-h-screen w-full px-6 py-6">
      <div className="max-w-2xl mx-auto w-full">
        <button
          onClick={() => navigate("/notes")}
          className="inline-flex items-center gap-1.5 text-sm text-gray-500 hover:text-gray-900 mb-4 bg-transparent border-0 cursor-pointer"
        >
          <ArrowLeft className="w-3.5 h-3.5" />
          {t("settings.back")}
        </button>

        <h1 className="text-xl font-semibold text-gray-900 mb-4">{t("settings.title")}</h1>

        {/* Top-level section tabs */}
        <div className="flex gap-1 mb-6 border-b border-gray-200">
          {([
            ["audio", t("settings.tab.audio")],
            ["models", t("settings.tab.models")],
            ["notifications", lang === "en" ? "Notifications" : "알림"],
            ["language", t("settings.tab.language")],
          ] as [typeof section, string][]).map(([key, label]) => (
            <button
              key={key}
              onClick={() => setSection(key)}
              className={`px-4 py-2 text-sm font-medium border-0 border-b-2 bg-transparent cursor-pointer transition-colors -mb-px ${
                section === key
                  ? "border-sky-600 text-sky-700"
                  : "border-transparent text-gray-500 hover:text-gray-900"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        {section === "audio" && <SourceSelector testMode="inline" />}

        {section === "notifications" && <NotificationSection />}

        {section === "language" && <LanguageSection />}

        {section === "models" && (
          <div>
            <p className="text-xs text-gray-500 mb-4">
              {t("settings.models.intro")}
            </p>

            {/* LLM / ASR sub-tabs + 새 모델 추가 */}
            <div className="flex items-center justify-between mb-5">
              <div className="inline-flex bg-white border border-gray-200 rounded-full p-1">
                {(["llm", "asr"] as EndpointKind[]).map((k) => (
                  <button
                    key={k}
                    onClick={() => {
                      setTab(k);
                      resetForm();
                    }}
                    className={`px-4 py-1.5 rounded-full text-sm font-medium border-0 cursor-pointer transition-colors ${
                      tab === k ? "bg-sky-600 text-white" : "bg-transparent text-gray-500 hover:text-gray-900"
                    }`}
                  >
                    {k === "llm" ? t("settings.kind.llm") : t("settings.kind.asr")}
                  </button>
                ))}
              </div>
              <button
                type="button"
                onClick={() => setAddOpen(true)}
                className="inline-flex items-center gap-1.5 px-4 py-2 rounded-lg bg-sky-600 text-white text-sm font-medium hover:bg-sky-700 border-0 cursor-pointer shadow-sm"
              >
                <Plus className="w-3.5 h-3.5" />
                {lang === "en" ? "Add model" : "새 모델 추가"}
              </button>
            </div>

            {loading ? (
          <div className="py-12 text-center">
            <Loader2 className="w-5 h-5 animate-spin text-gray-400 mx-auto" />
          </div>
        ) : (
          <div className="space-y-2.5">
            {endpoints.length === 0 && !showForm && (
              <div className="py-10 text-center text-sm text-gray-400 border border-dashed border-gray-200 rounded-xl">
                {t("settings.empty", { kind: tab.toUpperCase() })}
              </div>
            )}

            {endpoints.map((ep) => {
              const active = ep.is_active === 1;
              return (
                <div
                  key={ep.id}
                  onClick={() => {
                    if (!active) onActivate(ep.id);
                  }}
                  role="button"
                  aria-pressed={active}
                  title={active ? t("settings.model.inUse") : t("settings.model.clickUse")}
                  className={`rounded-xl border bg-white p-4 transition-colors ${
                    active
                      ? "border-sky-300 ring-1 ring-sky-100"
                      : "border-gray-100 hover:border-sky-200 hover:bg-sky-50/30 cursor-pointer"
                  }`}
                >
                  <div className="flex items-start gap-3">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <h3 className="text-sm font-medium text-gray-900 truncate">{ep.name}</h3>
                        {active ? (
                          <span className="inline-flex items-center gap-1 text-[10px] font-medium text-sky-700 bg-sky-50 border border-sky-100 rounded-full px-2 py-0.5">
                            <CheckCircle2 className="w-2.5 h-2.5" />
                            {t("settings.model.active")}
                          </span>
                        ) : (
                          <span className="text-[10px] text-gray-400">{t("settings.model.clickActivate")}</span>
                        )}
                      </div>
                      <p className="text-xs text-gray-500 mt-0.5 truncate">
                        {ep.model_id} · {ep.api_base_url}
                      </p>
                      {testResult?.id === ep.id && (
                        <p
                          className={`text-xs mt-1.5 inline-flex items-center gap-1 ${
                            testResult.result.success ? "text-emerald-600" : "text-red-600"
                          }`}
                        >
                          {testResult.result.success ? (
                            <CheckCircle2 className="w-3 h-3" />
                          ) : (
                            <XCircle className="w-3 h-3" />
                          )}
                          {testResult.result.message}
                          {testResult.result.response_time_ms != null &&
                            ` · ${testResult.result.response_time_ms}ms`}
                        </p>
                      )}
                    </div>
                    <div className="flex items-center gap-1 shrink-0" onClick={(e) => e.stopPropagation()}>
                      <IconBtn title={t("settings.action.test")} onClick={() => onTest(ep)} disabled={testingId === ep.id}>
                        {testingId === ep.id ? (
                          <Loader2 className="w-3.5 h-3.5 animate-spin" />
                        ) : (
                          <Zap className="w-3.5 h-3.5" />
                        )}
                      </IconBtn>
                      <IconBtn title={t("settings.action.edit")} onClick={() => onEdit(ep)}>
                        <Pencil className="w-3.5 h-3.5" />
                      </IconBtn>
                      <IconBtn title={t("settings.action.delete")} onClick={() => onDelete(ep.id)} danger>
                        <Trash2 className="w-3.5 h-3.5" />
                      </IconBtn>
                    </div>
                  </div>
                </div>
              );
            })}

            {showForm && (
              <form onSubmit={onSubmit} className="rounded-xl border border-gray-200 bg-white p-4 space-y-3">
                <Field label={t("settings.field.name")}>
                  <input
                    required
                    value={form.name}
                    onChange={(e) => setForm({ ...form, name: e.target.value })}
                    placeholder={tab === "llm" ? t("settings.field.name.ph.llm") : t("settings.field.name.ph.asr")}
                    className="form-input"
                  />
                </Field>
                <Field label={t("settings.field.modelId")}>
                  <input
                    required
                    value={form.model_id}
                    onChange={(e) => setForm({ ...form, model_id: e.target.value })}
                    placeholder={tab === "llm" ? "gpt-4o-mini" : "whisper-1"}
                    className="form-input"
                  />
                </Field>
                <Field label="API Base URL">
                  <input
                    required
                    value={form.api_base_url}
                    onChange={(e) => setForm({ ...form, api_base_url: e.target.value })}
                    placeholder="https://api.openai.com/v1"
                    className="form-input"
                  />
                </Field>
                <Field label="API Key">
                  <input
                    type="password"
                    value={form.api_key}
                    onChange={(e) => setForm({ ...form, api_key: e.target.value })}
                    placeholder="sk-…"
                    className="form-input"
                  />
                  <p className="text-[10px] text-gray-400 mt-1">
                    {t("settings.apiKey.note")}
                  </p>
                </Field>
                {tab === "asr" && (
                  <>
                    <Field label={t("settings.field.asrMode")}>
                      <select
                        value={form.request_mode}
                        onChange={(e) => setForm({ ...form, request_mode: e.target.value })}
                        className="form-input bg-white"
                      >
                        <option value="chat_completions">{t("settings.asrMode.chat")}</option>
                        <option value="transcriptions">{t("settings.asrMode.transcribe")}</option>
                      </select>
                    </Field>
                    <div className="flex gap-3">
                      <Field label={t("settings.field.chunk")}>
                        <input
                          value={form.chunk_seconds}
                          onChange={(e) => setForm({ ...form, chunk_seconds: e.target.value })}
                          placeholder={t("settings.field.chunk.ph")}
                          className="form-input"
                        />
                      </Field>
                      <Field label={t("settings.field.maxTokens")}>
                        <input
                          value={form.max_tokens}
                          onChange={(e) => setForm({ ...form, max_tokens: e.target.value })}
                          placeholder={t("settings.field.maxTokens.ph")}
                          className="form-input"
                        />
                      </Field>
                    </div>
                  </>
                )}
                <div className="flex items-center gap-2 pt-1">
                  <button
                    type="submit"
                    className="px-4 py-2 bg-sky-600 text-white rounded-lg text-sm font-medium hover:bg-sky-700 border-0 cursor-pointer"
                  >
                    {editingId ? t("settings.form.update") : t("settings.form.add")}
                  </button>
                  <button
                    type="button"
                    onClick={resetForm}
                    className="px-4 py-2 text-gray-500 hover:text-gray-900 text-sm bg-transparent border-0 cursor-pointer"
                  >
                    {t("common.cancel")}
                  </button>
                </div>
              </form>
            )}
              </div>
            )}
          </div>
        )}
      </div>

      {addOpen && (
        <AddModelModal kind={tab} onClose={() => setAddOpen(false)} onAdded={fetch} />
      )}
    </div>
  );
}


function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block flex-1">
      <span className="text-[11px] text-gray-500 mb-1 block">{label}</span>
      {children}
    </label>
  );
}

function IconBtn({
  children,
  onClick,
  title,
  disabled,
  danger,
}: {
  children: React.ReactNode;
  onClick: () => void;
  title: string;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      disabled={disabled}
      className={`p-1.5 rounded-md bg-transparent border-0 cursor-pointer transition-colors disabled:opacity-40 ${
        danger ? "text-gray-400 hover:text-red-600" : "text-gray-400 hover:text-gray-900"
      }`}
    >
      {children}
    </button>
  );
}
