export type ModelVendor =
  | "openai"
  | "anthropic"
  | "google"
  | "xai"
  | "deepseek"
  | "moonshot"
  | "qwen"
  | "meta"
  | "mistral"
  | "unknown";

const VENDOR_LABELS: Record<ModelVendor, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  google: "Google",
  xai: "xAI",
  deepseek: "DeepSeek",
  moonshot: "Moonshot",
  qwen: "通义千问",
  meta: "Meta",
  mistral: "Mistral",
  unknown: "未知供应商",
};

type VendorRule = {
  vendor: ModelVendor;
  test: (normalized: string) => boolean;
};

const FAMILY_RULES: VendorRule[] = [
  {
    vendor: "anthropic",
    test: (n) => n.includes("claude") || n.includes("anthropic"),
  },
  {
    vendor: "xai",
    test: (n) => n.includes("grok") || n.includes("xai"),
  },
  {
    vendor: "deepseek",
    test: (n) => n.includes("deepseek"),
  },
  {
    vendor: "moonshot",
    test: (n) => n.includes("kimi") || n.includes("moonshot"),
  },
  {
    vendor: "qwen",
    test: (n) => n.includes("qwen") || n.includes("qwq"),
  },
  {
    vendor: "google",
    test: (n) => n.includes("gemini") || n.includes("gemma") || n === "google",
  },
  {
    vendor: "meta",
    test: (n) => n.includes("llama") || n.includes("meta"),
  },
  {
    vendor: "mistral",
    test: (n) => n.includes("mistral") || n.includes("mixtral") || n.includes("codestral"),
  },
  {
    vendor: "openai",
    test: isOpenAiName,
  },
];

function isOpenAiName(normalized: string): boolean {
  if (
    normalized.includes("openai") ||
    normalized.includes("chatgpt") ||
    normalized.includes("gpt-") ||
    normalized.startsWith("gpt") ||
    normalized.includes("dall-e") ||
    normalized.includes("dalle") ||
    normalized.includes("whisper")
  ) {
    return true;
  }
  if (/^o[1-4]([.-]|$)/.test(normalized)) {
    return true;
  }
  return normalized.includes("codex") && !normalized.includes("claude");
}

function normalizeVendorInput(value: string): string {
  return value.trim().toLowerCase();
}

function matchVendor(normalized: string): ModelVendor {
  if (
    !normalized ||
    normalized === "其他" ||
    normalized.includes("未知") ||
    normalized.includes("未标注")
  ) {
    return "unknown";
  }
  for (const rule of FAMILY_RULES) {
    if (rule.test(normalized)) {
      return rule.vendor;
    }
  }
  return "unknown";
}

/** 按模型名识别供应商；识别不出时再回退到 provider 字段。 */
export function resolveModelVendor(name: string, provider?: string | null): ModelVendor {
  const fromName = matchVendor(normalizeVendorInput(name));
  if (fromName !== "unknown") {
    return fromName;
  }
  if (provider) {
    return matchVendor(normalizeVendorInput(provider));
  }
  return "unknown";
}

export function vendorLabel(vendor: ModelVendor): string {
  return VENDOR_LABELS[vendor];
}
