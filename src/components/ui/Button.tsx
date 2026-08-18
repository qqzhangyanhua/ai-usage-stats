import type { ButtonProps } from "./type";

export function Button({
  variant = "ghost",
  size = "md",
  className,
  children,
  type = "button",
  ...props
}: ButtonProps) {
  const variantClass =
    variant === "accent"
      ? "ghost-btn ghost-btn-accent"
      : variant === "danger"
        ? "ghost-btn ghost-btn-danger"
        : variant === "text"
          ? "text-btn"
          : variant === "icon"
            ? "icon-btn"
            : "ghost-btn";
  const sizeClass = variant === "text" || variant === "icon" || size === "md" ? "" : "ghost-btn-sm";
  const classes = [variantClass, sizeClass, className].filter(Boolean).join(" ");
  return (
    <button type={type} className={classes} {...props}>
      {children}
    </button>
  );
}
