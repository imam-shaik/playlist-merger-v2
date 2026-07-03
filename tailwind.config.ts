import type { Config } from "tailwindcss";

const config: Config = {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // Base surfaces
        bg: {
          base: "#08080a",
          surface: "#0f0f13",
          elevated: "#161619",
          overlay: "#1c1c21",
        },
        // Borders
        border: {
          subtle: "#1e1e26",
          DEFAULT: "#26262f",
          strong: "#35353f",
        },
        // Text
        text: {
          primary: "#eeeef5",
          secondary: "#a0a0b5",
          muted: "#5c5c75",
          disabled: "#3a3a4a",
        },
        // Accent – indigo-violet spectrum
        accent: {
          50: "#eef2ff",
          100: "#e0e7ff",
          200: "#c7d2fe",
          300: "#a5b4fc",
          400: "#818cf8",
          500: "#6366f1",
          600: "#4f46e5",
          700: "#4338ca",
          800: "#3730a3",
          900: "#312e81",
          DEFAULT: "#6366f1",
          muted: "#4f46e520",
        },
        // Cyan highlight
        highlight: {
          DEFAULT: "#22d3ee",
          muted: "#22d3ee15",
        },
        // Status colors
        success: {
          DEFAULT: "#10b981",
          muted: "#10b98115",
        },
        warning: {
          DEFAULT: "#f59e0b",
          muted: "#f59e0b15",
        },
        danger: {
          DEFAULT: "#ef4444",
          muted: "#ef444415",
        },
      },
      fontFamily: {
        sans: ["system-ui", "sans-serif"],
        mono: ["'JetBrains Mono'", "monospace"],
        display: ["system-ui", "sans-serif"],
      },
      fontSize: {
        "2xs": ["0.65rem", { lineHeight: "1rem" }],
      },
      borderRadius: {
        "2xs": "2px",
        xs: "4px",
        sm: "6px",
        DEFAULT: "8px",
        md: "10px",
        lg: "12px",
        xl: "16px",
        "2xl": "20px",
      },
      spacing: {
        px: "1px",
        0.5: "2px",
        1: "4px",
        1.5: "6px",
        2: "8px",
        2.5: "10px",
        3: "12px",
        3.5: "14px",
        4: "16px",
        5: "20px",
        6: "24px",
        7: "28px",
        8: "32px",
        9: "36px",
        10: "40px",
        11: "44px",
        12: "48px",
        14: "56px",
        16: "64px",
        18: "72px",
        20: "80px",
      },
      boxShadow: {
        glow: "0 0 20px rgba(99, 102, 241, 0.15)",
        "glow-sm": "0 0 10px rgba(99, 102, 241, 0.1)",
        "inner-glow": "inset 0 1px 0 rgba(255,255,255,0.05)",
        card: "0 1px 3px rgba(0,0,0,0.4), 0 0 0 1px rgba(255,255,255,0.04)",
        "card-hover":
          "0 4px 20px rgba(0,0,0,0.5), 0 0 0 1px rgba(99,102,241,0.2)",
        modal: "0 25px 50px rgba(0,0,0,0.8), 0 0 0 1px rgba(255,255,255,0.06)",
      },
      backdropBlur: {
        xs: "2px",
        sm: "4px",
        DEFAULT: "8px",
        md: "12px",
        lg: "16px",
        xl: "24px",
      },
      animation: {
        "fade-in": "fadeIn 0.2s ease-out",
        "slide-up": "slideUp 0.3s cubic-bezier(0.16, 1, 0.3, 1)",
        "slide-down": "slideDown 0.3s cubic-bezier(0.16, 1, 0.3, 1)",
        shimmer: "shimmer 2s infinite linear",
        pulse: "pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite",
        "spin-slow": "spin 3s linear infinite",
        "progress-indeterminate": "progressIndeterminate 1.5s ease-in-out infinite",
      },
      keyframes: {
        fadeIn: {
          "0%": { opacity: "0" },
          "100%": { opacity: "1" },
        },
        slideUp: {
          "0%": { opacity: "0", transform: "translateY(8px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
        slideDown: {
          "0%": { opacity: "0", transform: "translateY(-8px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
        shimmer: {
          "0%": { backgroundPosition: "-200% 0" },
          "100%": { backgroundPosition: "200% 0" },
        },
        progressIndeterminate: {
          "0%": { transform: "translateX(-100%)" },
          "100%": { transform: "translateX(400%)" },
        },
      },
    },
  },
  plugins: [],
};

export default config;
