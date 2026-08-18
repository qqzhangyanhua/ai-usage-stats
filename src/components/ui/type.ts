import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonVariant = "ghost" | "accent" | "danger" | "text" | "icon";

export type ButtonSize = "sm" | "md";

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
  children: ReactNode;
};
