import type { Config } from "tailwindcss";

const token = (name: string) => `hsl(var(--${name}) / <alpha-value>)`;

const config = {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        background: token("background"),
        foreground: token("foreground"),
        surface: token("surface"),
        "surface-subtle": token("surface-subtle"),
        "surface-inset": token("surface-inset"),
        popover: token("popover"),
        muted: token("muted"),
        "muted-foreground": token("muted-foreground"),
        border: token("border"),
        "border-strong": token("border-strong"),
        input: token("input"),
        ring: token("ring"),
        hover: token("hover"),
        selected: token("selected"),
        "selected-foreground": token("selected-foreground"),
        scrim: token("scrim"),
        "control-thumb": token("control-thumb"),
        "control-active": token("control-active"),
        "control-active-foreground": token("control-active-foreground"),
        "control-active-border": token("control-active-border"),
        "channel-health-surface": token("channel-health-surface"),
        "channel-health-label": token("channel-health-label"),
        "channel-health-foreground": token("channel-health-foreground"),
        "channel-health-emphasis": token("channel-health-emphasis"),
        "channel-health-bar": token("channel-health-bar"),
        "on-solid": token("on-solid"),
        primary: {
          DEFAULT: token("primary"),
          solid: token("primary-solid"),
          foreground: token("primary-foreground"),
        },
        success: {
          surface: token("success-surface"),
          foreground: token("success-foreground"),
          border: token("success-border"),
        },
        warning: {
          surface: token("warning-surface"),
          foreground: token("warning-foreground"),
          border: token("warning-border"),
        },
        danger: {
          surface: token("danger-surface"),
          foreground: token("danger-foreground"),
          border: token("danger-border"),
          solid: token("danger-solid"),
        },
        info: {
          surface: token("info-surface"),
          foreground: token("info-foreground"),
          border: token("info-border"),
        },
        metric: Object.fromEntries(
          ["slate", "emerald", "green", "blue", "amber", "indigo", "violet", "purple", "rose"].map((accent) => [
            accent,
            {
              surface: token(`metric-${accent}-surface`),
              foreground: token(`metric-${accent}-foreground`),
            },
          ]),
        ),
        platform: Object.fromEntries(
          ["anthropic", "openai", "gemini", "grok", "image", "generic"].map((platform) => [
            platform,
            {
              surface: token(`platform-${platform}-surface`),
              foreground: token(`platform-${platform}-foreground`),
              border: token(`platform-${platform}-border`),
            },
          ]),
        ),
      },
      boxShadow: {
        surface: "var(--surface-shadow)",
        "surface-hover": "var(--surface-shadow-hover)",
        popover: "var(--popover-shadow)",
        dialog: "var(--dialog-shadow)",
      },
      borderRadius: {
        lg: "8px",
        md: "6px",
        sm: "4px",
      },
    },
  },
  plugins: [],
} satisfies Config;

export default config;
