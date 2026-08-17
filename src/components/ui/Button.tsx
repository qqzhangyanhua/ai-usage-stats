import type { ButtonHTMLAttributes, ReactNode } from "react";

type ButtonVariant = "ghost" | "accent" | "danger" | "text" | "icon";

export function Button({
  variant = "ghost",
  className,
  children,
  type = "button",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  children: ReactNode;
}) {
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
  const classes = className ? `${variantClass} ${className}` : variantClass;
  return (
    <button type={type} className={classes} {...props}>
      {children}
    </button>
  );
}
