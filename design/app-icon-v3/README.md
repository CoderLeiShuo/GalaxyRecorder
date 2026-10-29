# 吉行影记 · 极简镜头

用户确认采用第 1 张效果图 `approved-preview.png`。通过内置 image_gen 工具提取正式图标，保留蓝色渐变、白色相机框、圆形镜头、录制圆点和右下方两段道路标记。

- `generated.png`：内置工具输出；去除效果图文字、外部背景、阴影和小图标，背景铺满方形。
- `master.png`：1024 × 1024 发布母版；桌面和常规安装图标由此生成。
- `adaptive-foreground.png`：母版居中缩放为 75%，为 Android 自适应遮罩预留空间；外部由同色背景填充。
- `background.png`：Android 自适应背景。
- `public/app-icon.png`：256 × 256 网页图标。设备页顶部仍不显示 Logo。

生成各平台图标并同步 Android 资源：`node tools/generate_icons.mjs`。此脚本现在默认使用本目录；此前的 v2 保留为设计历史。

内置工具的编辑提示词：

> Use case: precise-object-edit. Input image is the approved concept 01 极简镜头 for 吉行影记. Extract and prepare ONLY the large blue camera app icon as a production square launcher artwork. Preserve the exact approved design identity: the white rounded rectangular camera/viewfinder outline, the bold white circular lens ring with blue center slightly right of center, the small white recording dot at upper left inside the frame, and the two small slanted white road marks at the lower right INSIDE the camera outline. Keep the same clean proportions, stroke weights and cobalt blue to sky blue gradient feeling. Remove the presentation page, ALL text, ALL captions, small duplicate icon and external shadows. Extend the icon's blue gradient background to cover the entire square with square outer corners, no outer rounded tile or border, no white or transparent margin, no surrounding background. Platform masking will provide rounded corners later. The white camera symbol should be centered with comfortable padding, occupying about 70% of the canvas width and keeping the same wider-than-tall camera shape. Keep all symbol parts aligned exactly as in the approved reference; this is asset preparation, not a redesign. Crisp flat artwork, very clean edges, no new details, no 3D, no aperture blades, no reflections. Output only one square production icon.
