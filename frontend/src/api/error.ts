/** A failed backend call: a non-2xx response from the server, or a rejected Tauri command. */
export class ApiError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = "ApiError"
    this.status = status
  }
}
