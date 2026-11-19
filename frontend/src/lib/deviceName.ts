// Order matters: Edge, Samsung Internet and Opera also say "Chrome", and Chrome says "Safari".
const BROWSERS: [RegExp, string][] = [
  [/Edg(e|A|iOS)?\//, "Edge"],
  [/SamsungBrowser\//, "Samsung Internet"],
  [/OPR\/|Opera/, "Opera"],
  [/Firefox\/|FxiOS\//, "Firefox"],
  [/Chrome\/|CriOS\//, "Chrome"],
  [/Safari\//, "Safari"],
]

// Android says "Linux", and iPadOS can say "Macintosh".
const SYSTEMS: [RegExp, string][] = [
  [/Android/, "Android"],
  [/iPhone|iPod/, "iOS"],
  [/iPad/, "iPadOS"],
  [/CrOS/, "ChromeOS"],
  [/Windows/, "Windows"],
  [/Macintosh|Mac OS X/, "macOS"],
  [/Linux/, "Linux"],
]

const find = (list: [RegExp, string][], ua: string) => list.find(([re]) => re.test(ua))?.[1]

/**
 * What the browser's sign-in calls this device, such as "Firefox on Linux". Worked out here, so
 * the server only ever gets the name, never the user agent.
 */
export function browserDeviceName(userAgent: string): string {
  const browser = find(BROWSERS, userAgent) ?? "Browser"
  const system = find(SYSTEMS, userAgent)
  return system ? `${browser} on ${system}` : browser
}
