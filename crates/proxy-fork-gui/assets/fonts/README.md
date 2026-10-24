# Noto Sans SC

`NotoSansSC.ttf` contains static Regular (400) outlines, under the adjacent
[SIL Open Font License](OFL.txt). Upstream: [Google Fonts / Noto Sans SC](https://github.com/google/fonts/tree/main/ofl/notosanssc).

Dear ImGui's stb renderer does not apply variable-font weight axes. The upstream
variable font defaults to Thin (100), which is unsuitable for small light-theme
UI text. Generate the bundled instance with FontTools (a build-time tool only):

```sh
fonttools varLib.instancer 'NotoSansSC[wght].ttf' wght=400 --update-name-table --no-recalc-timestamp --output NotoSansSC.ttf
```

`tests/font_test.rs` checks the static-outline and Regular-weight contract.
