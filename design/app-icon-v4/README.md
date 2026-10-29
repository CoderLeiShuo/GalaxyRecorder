# 吉行影记 · 01 原版基准

用户确认采用最新九款细化图的左上角 01「原版基准」。`approved-preview.png` 保留原始对比图；通过内置 image_gen 编辑工具提取正式母版，保留蓝色渐变、带肩线的白色相机框、微光蓝色镜头、右上红色录制点和右下道路标记。

- `generated.png`：工具原始输出。
- `master.png`：1024 × 1024 方形发布母版，不包含文字、外部背景和阴影。
- `adaptive-foreground.png`：母版缩放到 75% 后居中留白，供 Android 自适应遮罩使用。
- `background.png`：Android 自适应背景。
- `public/app-icon.png`：网页图标。应用内左上角继续不显示 Logo。

执行 `node tools/generate_icons.mjs` 生成各平台图标并同步 Android 应用资源。该脚本当前默认使用 v4；旧目录保留为设计历史。

内置工具编辑提示词：

> Use case: precise-object-edit. The attached image is an app icon comparison sheet; the ONLY edit target is the TOP LEFT icon captioned 01 · 原版基准. Extract that exact approved icon into ONE production-ready square launcher artwork. Do not use any other variant. Preserve the approved identity and proportions: a white camera outline with a small raised centered shoulder on top, a substantial white circular lens ring surrounding a polished BLUE glass lens with a restrained highlight at upper left, a small RED recording dot inside the upper right of the camera, and the short white slanted roadway strokes integrated into the camera's lower-right edge. Keep the same confident sky-blue-to-cobalt gradient color feeling, exact white line weight, slightly dimensional blue lens, red dot location and recognizable camera silhouette. Asset preparation only, no redesign. Remove the comparison board, every caption/number, all other icons, the external background and drop shadow. Extend the blue tile gradient to fill the entire square canvas with square outer corners, no outer rounded-square frame, no white margins or transparency. Native launcher masks will create rounded corners later. Center the camera mark with comfortable padding, about 70 percent canvas width, fully intact including shoulder and road marks. No text, no titles, no new details, no realistic camera hardware, no aperture blades, no 3D extrusion, no extra glows. Output only the single high-resolution square icon, precisely matching concept 01.
