import { type RefObject, useEffect, useRef } from "react"

/** A ref to the latest `value`, for listeners attached once. */
export function useLatest<T>(value: T): RefObject<T> {
  const ref = useRef(value)
  useEffect(() => {
    ref.current = value
  })
  return ref
}
