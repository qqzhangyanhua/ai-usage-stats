/** 存储仍是每 Token 美元；界面按 USD / 1M tokens 展示和编辑。 */
export const TOKENS_PER_MILLION = 1_000_000;

export function toPerMillion(perToken: number): number {
  return perToken * TOKENS_PER_MILLION;
}

export function fromPerMillion(perMillion: number): number {
  return perMillion / TOKENS_PER_MILLION;
}

/** 给 number input 用，去掉浮点尾差（如 3.0000000000000004）。 */
export function formatPerMillionInput(perToken: number): string {
  if (!Number.isFinite(perToken) || perToken === 0) {
    return "0";
  }
  return String(Number(toPerMillion(perToken).toPrecision(12)));
}

/** 解析 USD/1M 输入；空串视为 0，非法或负数返回 null（调用方应忽略本次改动）。 */
export function parsePerMillionInput(raw: string): number | null {
  const trimmed = raw.trim();
  if (trimmed === "") {
    return 0;
  }
  const value = Number(trimmed);
  if (!Number.isFinite(value) || value < 0) {
    return null;
  }
  return fromPerMillion(value);
}
