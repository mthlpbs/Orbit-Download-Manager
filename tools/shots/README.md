# Windows help screenshots

The list in `shots.toml` drives every picture in `docs/images/help/`. On Windows, run:

```powershell
cargo build --release -p mdm-app -p mdm-labserver
python tools/shots/make.py
python tools/shots/make.py check
```

The tool launches its own app and test server with separate data folders, captures the listed
windows in light and dark mode, then crops and annotates the results. Raw captures stay in
`target/shots/raw/`. Use `make.py build` to rebuild images from existing captures, and
`--only add-url,scheduler` to limit the list.

The Docs screenshots workflow can also produce captures on a stock Windows runner. Download
its `docs-shots` artifact and run `python tools/shots/make.py docs <artifact-folder>`.

To add a screenshot, add an entry to `shots.toml`, add its matching Markdown image and caption
to the relevant docs page, and regenerate the pictures. `make.py check` validates references,
alt text, captions, unused files and the 300 KB image budget.
