/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        panel: "#111827",
        steel: "#1f2937",
      },
      boxShadow: {
        inspector: "0 18px 60px rgba(2, 6, 23, 0.36)",
      },
    },
  },
  plugins: [],
};
