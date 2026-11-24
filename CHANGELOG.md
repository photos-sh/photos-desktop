# Changelog

All notable changes to this project are written here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.4.0] - 2026-11-23

### Added

- **Edge shift**, **Edge hardness** and **Edge smoothing**, options of remove
  background in the batch and the editor, and of remove background in a
  recipe: they adjust the cutout's edge. *Edge shift* (-10 to 10 pixels)
  moves the edge, inward when negative and outward when positive; -1 to -2
  trims the light rim a dark product shows on a dark page. *Edge hardness*
  (0 to 100) makes the edge crisper, about a pixel wide at 100; on a dark
  page, pair it with a small negative shift. *Edge smoothing* (0 to 10)
  irons out ripples in the outline; from about 4 it rounds small corners and
  can break thin parts such as a wire stand. Each is 0 by default. At 0 the
  app sends nothing for it, so the request and the recipe are what they were
  in 1.3.0; otherwise it sends `shift`, `hardness` or `smooth` (in a recipe,
  the same keys in `[remove_background]`). A value outside its range is
  refused before anything is sent. The controls need the Photos.sh API of
  23 November 2026 or later; an older engine refuses a request that has one
  with a 400, and a recipe that has one with a 422.

## [1.3.0] - 2026-11-20

### Added

- **Model**, an option of remove background in the batch and the editor, and
  of remove background in a recipe: which model finds the subject. *Atlas*
  (the default) is for everyday product photos, a product alone or with its
  box, on any backdrop. *Lumen* is for pale packaging on a light backdrop,
  such as a cream or wooden box beside a bottle, and close-ups where the
  product fills the photo; it may keep a dark backdrop at the photo's edge,
  and can cut away part of an open frame or wire stand. *Muse* is for
  people: portraits, and models wearing the product. Each costs the same.
  With Atlas the app sends no `model`, so the service's default applies;
  otherwise it sends `model=lumen` or `model=muse` (in a recipe, `model` in
  `[remove_background]`). Choosing a model needs Photos.sh engine 0.1.14 or
  newer.

## [1.2.0] - 2026-11-16

### Added

- **Recipe**, in the batch and the editor: several steps on one upload, sent
  as one request and charged as one. Reads barcodes and text off the photo
  as it came, removes text, upscales a small photo, removes the background,
  frames or crops the product, adds a shadow and a background, and sets the
  size, format and file size. The editor shows what was read, with *Copy*.
  Reading barcodes and text needs a Photos.sh engine with reading steps in
  recipes; an older one refuses such a recipe with a 422.

## [1.1.0] - 2026-11-16

### Added

- **Updates from inside the app.** At start the app asks GitHub whether a
  newer version is out and offers it in a banner; *Install and restart*
  downloads it, checks its signature and installs it. *Settings → Updates*
  checks by hand, and a preference turns the check at start off. Nothing is
  installed while a batch runs.
- **Studio shot**, an option of remove background in the batch and the
  editor, for a product on a plain, light backdrop (as in a light tent): the
  cutout also keeps what stands out from the backdrop and touches the
  product, such as the gift box, case or lid shown with a bottle. Shadows and
  the floor below the product are left out. Off by default; when on, the app
  sends `studio=true`.

### Fixed

- The installers of 1.1.0 were named 1.0.0, and the app said it was 1.0.0:
  the version had not been raised. The version is now `Cargo.toml`'s alone
  (with `package.json` beside it), and a release tag that says otherwise is
  refused.
- When remove background finds no subject, the API returns the photo as it
  was. The file's line in a batch and the step in the editor now say "No
  subject found: nothing was cut out", not the crop's "No subject or face
  found: the crop is centred".

## [1.0.0] - 2026-11-13

The first version: Linux, macOS and Windows.

### Added

- **Batch.** Drop or pick photos and folders (folders are searched for JPEG,
  PNG, WebP, GIF, TIFF, BMP, HEIC/HEIF and AVIF files) and run one operation
  on all of them: remove background, upscale, remove text, resize / convert /
  compress, fix for a marketplace, or a preset. A few files are processed at
  once, with a status per file, overall progress, cancel and "retry failed".
- Cancel is immediate: the requests in flight are dropped (also while they
  wait to retry a busy server), no file is written for them, and they are
  marked "Cancelled", apart from "Failed" and "Not run". Retry runs them
  again. The app says what a cancelled photo may have cost: normally
  nothing, unless the service had just finished it. A running tool in the
  editor can be cancelled the same way, leaving its steps unchanged.
- Results go to a folder of your choice (by default a `photos-sh` folder
  beside the first photo), named after their sources with the extension of
  the result's type. Nothing is ever overwritten: a taken name gets a number.
- The cost of a batch in credits, and what is left, before it starts. A
  batch the balance cannot pay for is refused. Running out of credits or a
  refused key stops a batch instead of failing every remaining file.
- **Editor.** One photo, tool after tool: remove background (transparent or
  on a colour), upscale, remove text, erase (paint with a brush), resize /
  convert, fix for a marketplace, and check (quality findings and a
  marketplace's verdict). Undo and redo through the steps, a before/after
  slider against the original, transparency on a checkerboard, and "Save as"
  that replaces the original only after asking.
- The API's editing tools, in the batch and as tools of the editor: enhance
  (automatic, or brightness, contrast, saturation and sharpening), denoise,
  colourise, restore faces, hide faces (blur, pixelate or cover), crop to a
  shape around the subject or the faces, expand to a shape (continued by
  inpainting, or white), and a text watermark (position, colour, size,
  opacity). Remove background can put a contact or drop shadow under the
  product.
- **Vectorize** in the batch: the result is an SVG file, with a thumbnail in
  the list. It is not an editor tool, because an SVG is not a picture the
  next tool could work on.
- **Select object** in the editor: click an object to cut it out, with more
  clicks to add to it and a choice between the part clicked and the whole.
- What the new tools report is shown with each result: faces hidden or
  restored and those left alone, a crop that found no subject and is
  centred, an SVG's number of paths.
- A sentence of warning beside the options of the tools that can mislead:
  colourise guesses its colours, restore faces invents detail and is not for
  identity photos or evidence, denoise is slow without a GPU and takes
  photos up to 12 megapixels.
- **Settings.** The API key (paste, test, remove), the account and its
  credits, files processed at once, the default output format, and the API's
  base URL.
- The API key is kept in the system keychain (macOS Keychain, Windows
  Credential Manager, Secret Service on Linux). Without a keychain the key is
  kept in memory for the session only, and the app says so.
- Light and dark, following the system.

### Not in this version

Automatic updates, code signing and notarisation, watch folders, signing in
through the browser, the reading tools (barcodes, labels, identify, captions,
colour palettes, the list of faces), identity photos, a logo image as a
watermark, and server-side batch jobs.
