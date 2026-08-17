export function Spinner({ size = 18, className }: { size?: number; className?: string }) {
  return (
    <span
      className={["spinner", className].filter(Boolean).join(" ")}
      style={{ width: size, height: size }}
      role="status"
      aria-label="加载中"
    />
  );
}
