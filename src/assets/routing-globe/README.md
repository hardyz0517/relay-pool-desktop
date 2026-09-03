# Routing globe assets

Frozen 23.5 degree orthographic globe. The four sprites contain 16 horizontal frames and play over 2000ms with CSS `steps(16, end)`. Inactive uses a dedicated 0 degree front-facing frame at 122 degrees longitude. Active routing keeps the sprite animation running regardless of the system `prefers-reduced-motion` setting; off-screen and hidden documents pause the animation.

Regenerate with `pnpm generate:routing-globe-final-assets`.
