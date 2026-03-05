/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "#0f0f17",
        surface: "#1a1a2e",
        brand: "#7c3aed",
        success: "#10b981",
        amber: "#f59e0b",
        text: "#f9fafb",
        muted: "#6b7280"
      },
      fontFamily: {
        display: ["Syne", "sans-serif"],
        mono: ["IBM Plex Mono", "monospace"],
        sans: ["Inter", "sans-serif"]
      }
    }
  },
  plugins: []
};
