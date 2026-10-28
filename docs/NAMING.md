# Naming: how Lustro got its name

> **Decision (2026-10-28): the app is named _Lustro_** (working title before that: "System View"). It was chosen from the shortlist below.
>
> - **What it is:** Latin *lūstrō*, pronounced LOO-stroh. It means to survey and examine; to circle around and traverse; to purify; and to make bright. That covers the scan, orbiting the terrain, Cleanup and "show the work".
> - **How to write it:** "Lustro" in titles and UI. Use `lustro` for the repo name and as the lowercase identifier.
> - **Rust crates:** the crates.io name `lustro` is taken by an unrelated tiny CLI, so the workspace crates should be prefixed instead (`lustro-core`, `lustro-scan`, `lustro-app` and so on). They don't need to be published.
> - **Before launch:** do a proper trademark search. The existing LUSTRO marks are in housewares, tile and jewellery, not software.

_Compiled 2026-10-28. All checks were read-only: web searches, the GitHub and crates.io public APIs, and DNS lookups. Nothing was registered._

## The brief

In the user's words: a symbolic name, ideally from another language, that captures the whole app in one word, with bonus points if it's not an English word.

When asked, the user ranked three ideas the name should carry:

1. **Surveying and measuring**, the most important. The app measures every file on the drive.
2. **A bird's-eye view.** You see everything from above, as in the 3D terrain view.
3. **Tending it like a garden.** The app helps you curate, clean up and look after your drive.

The name should feel **warm and friendly**. A fully foreign word is fine, and any language is welcome, but sacred and Indigenous terms are off-limits.

## How to read the checks

- **GitHub:** every short word on the list is already a GitHub username, which is normal. That doesn't block us, because the repo would live under the user's account (`<user>/<name>`).
- **crates.io:** "free" means no published Rust crate has that name. The app doesn't need to publish one, but a free name avoids confusion.
- **Domains:** "resolves" means the domain points somewhere, so it's probably taken or parked. Registering one isn't needed to launch an open-source app.

---

## Top 3 and recommendation

| Rank | Name | One-line reason |
|---|---|---|
| **1 (recommended)** | **Lustro** | One Latin verb that covers survey, look over, circle around, review, purify and light up. That is the whole app, cleanup included, and it carries all three of the user's priorities. |
| 2 | **Kipimo** | Swahili for "measurement" and "measuring tool". It hits the top priority dead-on, sounds warm and playful, and turned up no conflicts at all. |
| 3 | **Varda** | Icelandic and Old Norse for a stone cairn: a waymark, a survey marker and a lookout post. It ties together surveying, the view from above and the app's pins. |

**Why Lustro.** The user asked for "one word that fully encapsulates the entirety of what this app is and can do." Lustro is the only candidate that covers all three priorities by its dictionary meaning rather than by stretching it:

- *Survey, examine, review:* the scan and the "What grew" view.
- *Move in a circle around, traverse:* orbiting the 3D terrain.
- *Purify:* Cleanup.
- *Make bright:* "show the work", meaning nothing is hidden.

It's easy to say, looks like the English word "lustre", and isn't an English word itself. The trade-off is that it's slightly less cuddly than Kipimo.

**If warmth matters most,** choose **Kipimo**. It's the friendliest-sounding name on the list and has the cleanest availability.

---

## Shortlist (10)

### 1. Lustro
- **Language:** Latin, *lūstrō* (a verb: "I survey, I purify").
- **Pronunciation:** LOO-stroh.
- **Literal meaning** ([Wiktionary](https://en.wiktionary.org/wiki/lustro)): to purify with an offering; to move in a circle around something; to traverse or roam; to review or examine (a military sense); to survey, observe, search or trace; to light up or make bright.
- **Why it fits:**
  - **Priority 1 (survey):** "to survey, observe, examine" is the scan, the folder tree and the "Where did my space go?" explainer.
  - **Priority 2 (view from above):** "to circle around, traverse" is orbiting the 3D terrain and following the dashed route.
  - **Priority 3 (tending):** "to purify" is the Cleanup review queue.
  - **Bonus:** "to make bright" matches the "show the work" principle.
- **In other languages:** Polish *lustro* means "mirror". That is a nice fit: the app reflects your drive back at you. Spanish and Portuguese *lustro* means "five-year period". Italian *lustro* means "shine". Esperanto *lustro* means "chandelier". None of these is negative.
- **Risks and conflicts:**
  - The [`lustro` crate](https://crates.io/crates/lustro) is taken by a small CLI with about 25 downloads that, coincidentally, also "surveys and illuminates". The Rust crates would therefore use names like `lustro-core`.
  - There is a [Polish mirror app on F-Droid](https://f-droid.org/pl/packages/com.polar.mirror/), and a Mac contacts exporter called Lustro existed around 2008 and is now defunct.
  - US trademarks for LUSTRO exist in housewares, tiles and jewellery ([summary of filings](https://markinton.com/trademark/lustro-99575850), [LUSTRO-WARE](https://trademark.justia.com/715/51/lustro-ware-71551362.html)). None of them covers software, but a proper trademark search is still worth doing before launch.
  - `lustro.com` and `lustro.app` both resolve.
  - It is close to Autodesk Lustre and the Lustre filesystem, but spelled differently.
- **In UI copy:** "Lustro found 12 GB that grew this week." · "Open Lustro" · Start menu entry **Lustro** · repo `lustro`.

### 2. Kipimo
- **Language:** Swahili, *kipimo* (plural *vipimo*).
- **Pronunciation:** kee-PEE-moh.
- **Literal meaning** ([Wiktionary](https://en.wiktionary.org/wiki/kipimo)): a measurement, or any tool used to measure something. It comes from *-pima*, "to measure", the same root as *upimaji wa ardhi*, "land surveying".
- **Why it fits:** It is priority 1 in a single word: the app is a measuring tool for your drive. The repeated vowels sound friendly and a little playful, which suits the "calm, warm" tone.
- **Risks and conflicts:**
  - Nothing came up in software, apps or companies ([search](https://www.google.com/search?q=%22Kipimo%22+app)).
  - The crates.io name is **free**.
  - There are a few small personal GitHub repos named `kipimo`.
  - `kipimo.com` resolves; `kipimo.app` doesn't seem to be taken.
  - Swahili is a living, widely spoken language, and *kipimo* is an everyday noun with no sacred meaning.
  - The main limitation is that it says nothing about the view from above or about tending. The About screen would have to tell that part of the story.
- **In UI copy:** "Kipimo found 12 GB that grew this week." · "Open Kipimo" · repo `kipimo`.

### 3. Varda
- **Language:** Icelandic *varða* and Old Norse *varða*, written without the ð. The related Norwegian and Danish word is *varde*.
- **Pronunciation:** VAR-dah.
- **Literal meaning** ([Wiktionary](https://en.wiktionary.org/wiki/var%C3%B0a)): a cairn or stone marker. In Old Norse it was a guard post marked by a cairn, used as a road or guide marker or as a platform for a beacon or **lookout**.
- **Why it fits:**
  - Cairns are survey and trail markers (priority 1) and lookout points (priority 2).
  - The terrain's **pins** (largest file, top grower) are exactly cairns: markers placed on the land so you can find your way.
  - Cairns are also built and tended by the people who pass by, a gentle link to priority 3.
- **Risks and conflicts:**
  - [Varda Space Industries](https://job-boards.greenhouse.io/vardaspace/jobs/7786907003) is a well-funded space-manufacturing company, so the name is well known.
  - Varda is a goddess in Tolkien's legendarium, and Agnès Varda was a French filmmaker.
  - In Icelandic, *varða* is also a verb meaning "to concern", which is harmless.
  - No disk, file or storage software uses the name.
  - The crates.io name is **free**, there are many unrelated GitHub repos called `varda`, and both `.com` and `.app` resolve.
- **In UI copy:** "Varda found 12 GB that grew this week." · "Open Varda".

### 4. Tenbo
- **Language:** Japanese, 展望 *tenbō*.
- **Pronunciation:** TEN-boh.
- **Literal meaning:** a view or prospect from a height, and figuratively an outlook on what lies ahead. An observation deck is 展望台 *tenbōdai*.
- **Why it fits:** It is priority 2 exactly: the summit view of your drive. The "outlook" sense also covers "What grew" and trends over time. It sounds short and warm.
- **Risks and conflicts:**
  - No product uses "Tenbo" itself ([search](https://www.google.com/search?q=%22Tenbo%22+app)).
  - It is one letter away from **Tembo**, the name of a UK savings app ([Tembo Money](https://help.tembomoney.com/)) and an MSF learning app. *Tembo* is also Swahili for "elephant", which is a cute coincidence ("an elephant never forgets" what grew).
  - Writing the macron (Tenbō) is optional.
  - The crates.io name is free; both domains resolve.
- **In UI copy:** "Tenbo found 12 GB that grew this week."

### 5. Saikei
- **Language:** Japanese, 栽景 *saikei*.
- **Pronunciation:** sigh-KAY.
- **Literal meaning:** "planted landscape": a living miniature landscape of trees, rocks and terrain in a tray, tended like a garden ([Wikipedia](https://en.wikipedia.org/wiki/Saikei)). It is a modern art form, developed by Toshio Kawamoto in the 1960s.
- **Why it fits:** It is priority 3 combined with the terrain. Your drive is a small landscape you look down on and tend. It matches the logo, a hill on a disc, very closely.
- **Risks and conflicts:** No software, apps or companies use it ([search](https://www.google.com/search?q=%22Saikei%22+app)). The crates.io name is free, `saikei.app` doesn't seem to be taken, and `.com` resolves. The weakness is that measuring isn't part of the meaning.
- **In UI copy:** "Saikei found 12 GB that grew this week."

### 6. Bonkei
- **Language:** Japanese, 盆景 *bonkei*.
- **Pronunciation:** BONE-kay.
- **Literal meaning:** "tray landscape": a three-dimensional landscape in miniature, with mountains, valleys and water, made in a shallow tray ([Wikipedia](https://en.wikipedia.org/wiki/Bonkei)). It is the sculpted cousin of bonsai.
- **Why it fits:** It is almost a literal description of the terrain view: a miniature 3D landscape seen from above and lovingly kept.
- **Risks and conflicts:** It's clean in software, the crates.io name is free, and `bonkei.app` doesn't seem to be taken. However, English speakers may hear the slang word "bonk", which invites jokes. That makes it weaker than Saikei.
- **In UI copy:** "Bonkei found 12 GB that grew this week."

### 7. Groma
- **Language:** Latin, *groma* (from Greek *gnōmōn*).
- **Pronunciation:** GROH-mah.
- **Literal meaning:** the Roman land-surveyor's instrument, a staff with a rotating cross and plumb lines used to lay out straight lines and grids ([Wikipedia](https://en.wikipedia.org/wiki/Groma_(surveying))).
- **Why it fits:** It is priority 1 in its purest form: the tool that measured out the land. The grid even echoes the treemap.
- **Risks and conflicts:** **Medium.** **GROMA** is already geodetic surveying software from the Czech firm Geoline, which is the same semantic field and is software. There's also a [Groma grocery app](https://www.producthunt.com/products/groma/alternatives). The crates.io name is free, and `groma.app` doesn't seem to be taken.
- **In UI copy:** "Groma found 12 GB that grew this week."

### 8. Mitta
- **Language:** Finnish, *mitta*.
- **Pronunciation:** MIT-tah.
- **Literal meaning** ([Wiktionary](https://en.wiktionary.org/wiki/mitta)): a measure, a dimension, a gauge or measuring tape, or a standard. In Pali, *mitta* means "friend", which is a lovely bonus for "warm".
- **Why it fits:** It covers priority 1, it's very short, and it's friendly.
- **Risks and conflicts:** **Medium.** [Mitta Group](https://www.cbinsights.com/company/mitta-group/) (Mitta Oy and Mitta AB) is a Finnish-Swedish **surveying** and geotechnical engineering firm with about 400 staff. It isn't software, but it works in the same field. The crates.io name is free; both domains resolve.
- **In UI copy:** "Mitta found 12 GB that grew this week."

### 9. Kilele
- **Language:** Swahili, *kilele*.
- **Pronunciation:** kee-LEH-leh.
- **Literal meaning:** summit, peak, top, or high point.
- **Why it fits:** Priority 2, the summit view, and it matches the logo's pale-blue summit contour. It's very warm and musical.
- **Risks and conflicts:** No software or apps use it ([search](https://www.google.com/search?q=%22Kilele%22+app)). The crates.io name is free; both domains resolve. It's a little close to "Kile", KDE's LaTeX editor. The weakness is that it describes the mountain rather than measuring it.
- **In UI copy:** "Kilele found 12 GB that grew this week."

### 10. Hakari
- **Language:** Japanese, 秤 or 計り *hakari*.
- **Pronunciation:** hah-KAH-ree.
- **Literal meaning:** a scale or balance for weighing; a measure. It comes from *hakaru*, "to measure or weigh".
- **Why it fits:** It covers priority 1, weighing what takes up space.
- **Risks and conflicts:** **High in the Rust world.** [`hakari`](https://crates.io/crates/hakari) is a widely used Rust crate (cargo-hakari, about 5.5 million downloads), and Rust is the language this app is written in. Hakari is also a character in *Jujutsu Kaisen*. In Māori, *hākari* means "feast", an everyday word but still a Māori one. It's listed only for completeness and is not recommended.

---

## Rejected, and why

| Word | Language and meaning | Why rejected |
|---|---|---|
| Fukan | Japanese 俯瞰, bird's-eye view | Sounds vulgar in English. |
| Chōkan | Japanese 鳥瞰, bird's-eye view ("bird glance") | Reads as "choke-an". Awkward. |
| Sukat | Tagalog, measure or size | Reads as "suck at". |
| Ichibo | Japanese 一望, a sweeping view | *Ichibo* is also a beef rump cut. |
| Ichimoku | Japanese 一目, a single glance | Name of a well-known trading-chart indicator. |
| Yama | Japanese 山, mountain | Also the Hindu god of death. |
| Vaara | Finnish, wooded hill | Also means "danger" in Finnish. |
| Cwm | Welsh, valley | Hard to say, and sounds vulgar in English. |
| Mitori | Japanese 見取り(図), sketch map | Homophone of 看取り, caring for the dying. |
| Orama | Greek, sight or view | Taken by the Orama search engine. |
| Sansui | Japanese 山水, mountain-water landscape | Established audio brand. |
| Tunturi | Finnish, fell or upland | Established fitness-equipment brand. |
| Ramani | Swahili, map | Kenyan geospatial firm, ramani.io, and the `ramani-maps` library. Too crowded in mapping. |
| Hakoniwa | Japanese 箱庭, box garden | The TOPPERS Hakoniwa simulation platform and Hakoniwa Lab. Also long. |
| Khipu / Quipu | Quechua, knotted-cord records | KDE already has an app called Khipu, and the word is culturally significant. |
| Mensor | Latin, surveyor | Mensor is an instrument brand of WIKA. |
| Satoyama | Japanese 里山, tended foothill landscape | A lovely idea, but long and tied to a UN initiative. |
| Chikei | Japanese 地形, terrain | Accurate, but it feels cold and technical, not warm. |
| Utsikt / Overblik / Yfirsýn | Norwegian/Swedish, Danish, Icelandic: view, overview | Accurate, but they read as office words, not warm ones. Yfirsýn is hard to spell. |
| Vidde | Norwegian, wide high plateau | Weak link to measuring. |
| Seiri / Danshari | Japanese 整理 tidying, 断捨離 decluttering | Only about cleanup, and decluttering is the wrong tone. |
| Hlidskjalf | Old Norse, Odin's high seat that sees all worlds | Perfect meaning, but impossible to spell. |
| Conspectus, Summa, Libra, Vista, Census, Topos, Atlas | Latin and Greek: overview, total, scales, view, place, map | Already English words or heavily used names. |
| Panopsis | Greek coinage, all-seeing | Sounds like "panopticon", which suggests surveillance. That's wrong for a privacy-first app. |
| Isohypse / Dioptra | Greek, contour line / surveying instrument | Too technical to say. |
| Maunga, Mauna, Apu, Pacha, Inukshuk, Ahupuaʻa | Māori, Hawaiian, Quechua, Inuit, Hawaiian: mountain, mountain spirit, earth, cairn, land division | **Sacred or culturally significant** Indigenous terms. Avoided out of respect. |
| Kshetra, Darśana, Bhūmi, Mandala | Sanskrit: field, sacred sight, earth goddess, sacred diagram | Religious meanings. |
| Kuntur | Quechua, condor | Sacred animal in Andean cosmology. |
| Peta | Indonesian/Malay, map | Clashes with PETA. |
| Tobi, Kotka, Tai | Japanese, Finnish, Swahili: kite (bird), eagle, eagle | Too far from the app's purpose (and Kotka is a city). |
| Niwa, Hortus, Pardis, Bustani | Japanese, Latin, Persian, Swahili: garden | Only about the garden, and generic. |
| Kartta, Chizu, Harita | Finnish, Japanese, Turkish: map | Generic "map". |
| Tinjau | Indonesian, to survey or look out over | Spelling and pronunciation are unclear to English speakers. |
| Arolwg, Mesur | Welsh: survey, measure | Hard to pronounce. |
| Fjall, Hæð, Mlima, Tepetl, Oros, Kumpu, Huippu, Zirve, Mendi | Mountain, hill and summit words in Icelandic, Swahili, Nahuatl, Greek, Finnish, Turkish and Basque | Weaker than Kilele or Varda, harder to say, or already crowded. |

_That's 60+ candidates considered in total, counting the shortlist._
