import type { PriceEntry } from "../types";

/**
 * 常见 provider 官方公开单价预设，价格单位为「每百万 Token 美元」（更符合官网价目表的常见写法），
 * 导入时再换算成 PriceEntry 实际存储的「每 Token 美元」。
 *
 * 价格来自各官网 2026-08 前后的公开价目，仅作为配置起点；AI 模型定价变化频繁，
 * 导入后请对照官方最新价目自行核对/调整，尤其是限时优惠、阶梯计价、峰谷计价这类场景。
 */
export type PricePreset = {
  id: string;
  providerLabel: string;
  model: string;
  displayName: string;
  /** 每百万 Token 美元。 */
  inputPerM: number;
  outputPerM: number;
  cacheReadPerM: number;
  cacheWritePerM: number;
  asOf: string;
  note?: string;
};

export const PRICE_PRESETS: PricePreset[] = [
  // OpenAI —— 对应 Codex / Pi 等以 gpt- 开头的模型
  {
    id: "openai-gpt-5.6-sol",
    providerLabel: "OpenAI",
    model: "gpt-5.6-sol",
    displayName: "GPT-5.6 Sol",
    inputPerM: 5,
    outputPerM: 30,
    cacheReadPerM: 0.5,
    cacheWritePerM: 6.25,
    asOf: "2026-08-16",
  },
  {
    id: "openai-gpt-5.6-terra",
    providerLabel: "OpenAI",
    model: "gpt-5.6-terra",
    displayName: "GPT-5.6 Terra",
    inputPerM: 2,
    outputPerM: 12,
    cacheReadPerM: 0.2,
    cacheWritePerM: 2.5,
    asOf: "2026-08-16",
  },
  {
    id: "openai-gpt-5.6-luna",
    providerLabel: "OpenAI",
    model: "gpt-5.6-luna",
    displayName: "GPT-5.6 Luna",
    inputPerM: 0.2,
    outputPerM: 1.2,
    cacheReadPerM: 0.02,
    cacheWritePerM: 0.25,
    asOf: "2026-08-16",
  },
  {
    id: "openai-gpt-5.5",
    providerLabel: "OpenAI",
    model: "gpt-5.5",
    displayName: "GPT-5.5",
    inputPerM: 5,
    outputPerM: 30,
    cacheReadPerM: 0.5,
    cacheWritePerM: 6.25,
    asOf: "2026-08-16",
  },
  {
    id: "openai-gpt-5.4",
    providerLabel: "OpenAI",
    model: "gpt-5.4",
    displayName: "GPT-5.4",
    inputPerM: 2.5,
    outputPerM: 15,
    cacheReadPerM: 0.25,
    cacheWritePerM: 3.125,
    asOf: "2026-08-16",
  },
  {
    id: "openai-gpt-5.3-codex",
    providerLabel: "OpenAI",
    model: "gpt-5.3-codex",
    displayName: "GPT-5.3 Codex",
    inputPerM: 1.75,
    outputPerM: 14,
    cacheReadPerM: 0.175,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
  },

  // Anthropic —— 对应 Claude Code 等以 claude- 开头的模型
  {
    id: "anthropic-claude-fable-5",
    providerLabel: "Anthropic",
    model: "claude-fable-5",
    displayName: "Claude Fable 5",
    inputPerM: 10,
    outputPerM: 50,
    cacheReadPerM: 1,
    cacheWritePerM: 12.5,
    asOf: "2026-08-16",
  },
  {
    id: "anthropic-claude-opus-5",
    providerLabel: "Anthropic",
    model: "claude-opus-5",
    displayName: "Claude Opus 5",
    inputPerM: 5,
    outputPerM: 25,
    cacheReadPerM: 0.5,
    cacheWritePerM: 6.25,
    asOf: "2026-08-16",
  },
  {
    id: "anthropic-claude-sonnet-5",
    providerLabel: "Anthropic",
    model: "claude-sonnet-5",
    displayName: "Claude Sonnet 5",
    inputPerM: 2,
    outputPerM: 10,
    cacheReadPerM: 0.2,
    cacheWritePerM: 2.5,
    asOf: "2026-08-16",
    note: "官网标注为限时价，2026-08-31 后可能调整",
  },
  {
    id: "anthropic-claude-haiku-4.5",
    providerLabel: "Anthropic",
    model: "claude-haiku-4.5",
    displayName: "Claude Haiku 4.5",
    inputPerM: 1,
    outputPerM: 5,
    cacheReadPerM: 0.1,
    cacheWritePerM: 1.25,
    asOf: "2026-08-16",
  },

  // DeepSeek —— 对应 dsh 的模型
  {
    id: "deepseek-v4-flash",
    providerLabel: "DeepSeek",
    model: "deepseek-v4-flash",
    displayName: "DeepSeek V4 Flash",
    inputPerM: 0.22,
    outputPerM: 0.66,
    cacheReadPerM: 0.007,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
    note: "官网为峰谷两档计价，此处取谷时价，峰时约为 2 倍",
  },
  {
    id: "deepseek-v4-pro",
    providerLabel: "DeepSeek",
    model: "deepseek-v4-pro",
    displayName: "DeepSeek V4 Pro",
    inputPerM: 0.66,
    outputPerM: 1.98,
    cacheReadPerM: 0.022,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
    note: "官网为峰谷两档计价，此处取谷时价，峰时约为 2 倍",
  },

  // Google Gemini —— 对应 gemini 适配器的模型
  {
    id: "gemini-3.1-pro",
    providerLabel: "Google",
    model: "gemini-3.1-pro-preview",
    displayName: "Gemini 3.1 Pro",
    inputPerM: 2,
    outputPerM: 12,
    cacheReadPerM: 0.2,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
    note: "超过 200K 上下文价格翻倍",
  },
  {
    id: "gemini-3.6-flash",
    providerLabel: "Google",
    model: "gemini-3.6-flash",
    displayName: "Gemini 3.6 Flash",
    inputPerM: 1.5,
    outputPerM: 7.5,
    cacheReadPerM: 0.15,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
  },
  {
    id: "gemini-2.5-pro",
    providerLabel: "Google",
    model: "gemini-2.5-pro",
    displayName: "Gemini 2.5 Pro",
    inputPerM: 1.25,
    outputPerM: 10,
    cacheReadPerM: 0.125,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
  },
  {
    id: "gemini-2.5-flash",
    providerLabel: "Google",
    model: "gemini-2.5-flash",
    displayName: "Gemini 2.5 Flash",
    inputPerM: 0.3,
    outputPerM: 2.5,
    cacheReadPerM: 0.03,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
  },
  {
    id: "gemini-2.5-flash-lite",
    providerLabel: "Google",
    model: "gemini-2.5-flash-lite",
    displayName: "Gemini 2.5 Flash-Lite",
    inputPerM: 0.1,
    outputPerM: 0.4,
    cacheReadPerM: 0.01,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
  },

  // xAI —— 对应 grok 适配器的模型
  {
    id: "xai-grok-4.6",
    providerLabel: "xAI",
    model: "grok-4.6",
    displayName: "Grok 4.6",
    inputPerM: 2,
    outputPerM: 6,
    cacheReadPerM: 0.5,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
    note: "超过 200K 上下文价格翻倍",
  },
  {
    id: "xai-grok-4.5",
    providerLabel: "xAI",
    model: "grok-4.5",
    displayName: "Grok 4.5",
    inputPerM: 2,
    outputPerM: 6,
    cacheReadPerM: 0.3,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
    note: "超过 200K 上下文价格翻倍",
  },
  {
    id: "xai-grok-4.3",
    providerLabel: "xAI",
    model: "grok-4.3",
    displayName: "Grok 4.3",
    inputPerM: 1.25,
    outputPerM: 2.5,
    cacheReadPerM: 0.2,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
  },

  // Moonshot —— 对应 kimi 适配器（本机 wire.jsonl 常不暴露具体模型名，需手动核对/补全 model 字段）
  {
    id: "moonshot-kimi-k3",
    providerLabel: "Moonshot",
    model: "kimi-k3",
    displayName: "Kimi K3",
    inputPerM: 3,
    outputPerM: 15,
    cacheReadPerM: 0.3,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
  },
  {
    id: "moonshot-kimi-k2.6",
    providerLabel: "Moonshot",
    model: "kimi-k2.6",
    displayName: "Kimi K2.6",
    inputPerM: 0.95,
    outputPerM: 4,
    cacheReadPerM: 0.16,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
  },
  {
    id: "moonshot-kimi-k2.5",
    providerLabel: "Moonshot",
    model: "kimi-k2.5",
    displayName: "Kimi K2.5",
    inputPerM: 0.6,
    outputPerM: 3,
    cacheReadPerM: 0.1,
    cacheWritePerM: 0,
    asOf: "2026-08-16",
  },
];

function normalize(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/** 在本地已观测到的模型名里找最贴近该预设的一个，找不到则返回 null。 */
export function matchObservedModel(preset: PricePreset, observedModels: string[]): string | null {
  const target = normalize(preset.model);
  if (!target) {
    return null;
  }
  let contains: string | null = null;
  for (const observed of observedModels) {
    const candidate = normalize(observed);
    if (!candidate) {
      continue;
    }
    if (candidate === target) {
      return observed;
    }
    if (contains === null && (candidate.includes(target) || target.includes(candidate))) {
      contains = observed;
    }
  }
  return contains;
}

/** 把预设换算成实际写入单价表的 PriceEntry（modelOverride 用于替换成本地实测的模型名）。 */
export function presetToPriceEntry(preset: PricePreset, modelOverride?: string): PriceEntry {
  return {
    model: modelOverride ?? preset.model,
    provider: null,
    input: preset.inputPerM / 1_000_000,
    output: preset.outputPerM / 1_000_000,
    cache_read: preset.cacheReadPerM / 1_000_000,
    cache_creation: preset.cacheWritePerM / 1_000_000,
  };
}

export function groupPresetsByProvider(presets: PricePreset[]): Array<[string, PricePreset[]]> {
  const groups = new Map<string, PricePreset[]>();
  for (const preset of presets) {
    const list = groups.get(preset.providerLabel) ?? [];
    list.push(preset);
    groups.set(preset.providerLabel, list);
  }
  return Array.from(groups.entries());
}
