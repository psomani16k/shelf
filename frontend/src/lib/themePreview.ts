import fruit from "@/assets/theme-preview/fruit.svg"
import sunset from "@/assets/theme-preview/sunset.svg"
import { readerCss } from "@/lib/appearance"

// GitHub's light syntax colours, as a book about code might ship them.
const KEYWORD = "#cf222e"
const FUNCTION = "#8250df"
const STRING = "#0a3069"
const COMMENT = "#6e7781"
const NUMBER = "#0550ae"
const TYPE = "#953800"

const c = (color: string, text: string) => `<span style="color: ${color}">${text}</span>`

const CODE = [
  `${c(COMMENT, "// Lights the lamp, then waits for dawn.")}`,
  `${c(KEYWORD, "async fn")} ${c(FUNCTION, "keep_watch")}(lamp: &amp;${c(TYPE, "Lamp")}) -&gt; ` +
    `${c(TYPE, "Result")}&lt;()&gt; {`,
  `    lamp.${c(FUNCTION, "light")}(${c(NUMBER, "1200")}).${c(KEYWORD, "await")}?;`,
  `    ${c(KEYWORD, "let")} log = ${c(STRING, '"lit at dusk"')};`,
  `    ${c(FUNCTION, "println!")}(${c(STRING, '"{log}"')});`,
  `    ${c(TYPE, "Ok")}(())`,
  `}`,
].join("\n")

const CHIPS = [
  ["#e53935", "Red"],
  ["#fb8c00", "Orange"],
  ["#fdd835", "Yellow"],
  ["#43a047", "Green"],
  ["#1e88e5", "Blue"],
  ["#8e24aa", "Purple"],
]

// The book's own styles; the reader's go first, as in the reader.
const BOOK_CSS = `
  body { font-family: Georgia, serif; line-height: 1.5; margin: 0; padding: 16px 20px; }
  h1 { font-family: system-ui, sans-serif; margin: 0 0 4px; font-size: 1.5em; }
  .sub { margin-top: 0; color: #555; font-style: italic; }
  a { color: #1a5fb4; }
  figure { margin: 16px 0; text-align: center; }
  img { width: 100%; max-width: 360px; border-radius: 4px; }
  figcaption { font-size: 0.85em; color: #666; }
  blockquote { margin: 16px 0; padding: 4px 12px; border-left: 4px solid #c9a227;
    background: #fbf6e6; }
  pre { background: #f6f8fa; border: 1px solid #d0d7de; border-radius: 6px; padding: 10px 12px;
    font-size: 0.8em; overflow-x: auto; }
  table { border-collapse: separate; border-spacing: 4px; margin: 0 -4px; }
  td { padding: 4px 8px; border-radius: 4px; color: #fff; font: 600 0.75em system-ui, sans-serif; }
`

/** A sample book page for the theme builder's preview: text, colours, code and pictures. */
export const PREVIEW_HTML = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<style>${readerCss()}</style>
<style>${BOOK_CSS}</style>
</head>
<body>
<h1>The Lighthouse Keeper</h1>
<p class="sub">Chapter 3 · <span style="color: #8a5a00">An unexpected visitor</span></p>
<p>The storm came in <em>faster</em> than anyone expected. By nightfall the <strong>harbour
lights</strong> were all that showed from the cliff, and the radio crackled with
<a href="#">warnings from the coast guard</a>.</p>
<figure>
<img src="${sunset}" alt="A sunset over the bay, with a small sailing boat">
<figcaption>Fig. 1: Sunset over the bay.</figcaption>
</figure>
<p><span style="color: #c62828; font-weight: bold">Danger:</span> the lower stairs flood at
high tide. <span style="color: #2e7d32; font-weight: bold">Safe:</span> the lamp room stays dry.
<mark>Remember to wind the clock.</mark></p>
<blockquote>“Keep the light burning, whatever happens.”</blockquote>
<pre><code>${CODE}</code></pre>
<table><tr>${CHIPS.map(([bg, name]) => `<td style="background: ${bg}">${name}</td>`).join("")}</tr>
</table>
<figure>
<img src="${fruit}" alt="A blue bowl of fruit on a table">
<figcaption>Fig. 2: Still life with fruit.</figcaption>
</figure>
<p>She wrote the night's last entry by the light of the lamp, and went down to sleep.</p>
</body>
</html>`
