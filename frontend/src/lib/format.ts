/** `1 book`, `2 books` */
export const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`

export const DATE_TIME = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
})

/** `name` inside `folder`; `""` is the root. */
export const joinPath = (folder: string, name: string) => (folder ? `${folder}/${name}` : name)
