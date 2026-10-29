# 吉行影记 · 镜头与道路

使用内置 image_gen 工具生成（logo-brand），保留原始输出 `generated.png`；`master.png` 为等比缩小的 1024 × 1024 发布母版，`public/app-icon.png` 为应用内轻量图标。Tauri icon 命令从同一母版生成各平台图标，Android 前景使用 85% 的居中留白母版，为自适应遮罩留出空间。原记录仪图片不变。

生成提示词：

> Create a single production-ready app launcher icon for 吉行影记, a sophisticated dashcam video companion for Geely car owners. Square 1:1 raster icon, full bleed midnight indigo background with a very subtle cobalt gradient, absolutely no outer rounded-square frame, no external shadow or white margin. One beautifully resolved bold geometric symbol: a camera lens/aperture ring in luminous ice blue and electric cobalt, with an elegant forward road narrowing to a vanishing point within the lens, suggesting driving and recording. Symmetrical, disciplined, memorable premium technology identity. The road is formed from two clear tapering strokes and one small center lane mark, with a very subtle forward motion feeling. Big simple shapes and crisp edges, readable at 24px; minimal controlled highlight, mostly flat with restrained depth. Place the entire essential glyph inside the central 64 percent of the square so Android adaptive circular masks never crop the logo. Clean negative space around the glyph. Palette strictly deep indigo, vibrant blue, pale ice cyan. No words, no letters, no numerals, no Geely corporate badge, no realistic physical camera, no photographic textures, no busy circuit traces, no multiple concepts, no presentation mockup. Output only the finished icon artwork.

重新生成平台尺寸：`node tools/generate_icons.mjs`，会同时更新 `src-tauri/icons/` 和 Android 工程的图标资源。图标前景的留白已保存在 `adaptive-foreground.png` 中；生成器不会改动其余业务资源。
