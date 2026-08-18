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
  if (perToken === 0) {
    return "0";
  }
  return String(Number(toPerMillion(perToken).toPrecision(12)));
}
