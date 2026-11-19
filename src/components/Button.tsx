import type { ButtonHTMLAttributes } from "react";
import "./Button.css";

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export function Button({
  variant = "secondary",
  size = "md",
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: "sm" | "md";
}) {
  return (
    <button
      className={`pd-button pd-button-${variant} pd-button-${size}`}
      {...props}
    >
      {children}
    </button>
  );
}
