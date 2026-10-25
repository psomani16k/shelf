import { BookOpen } from "lucide-react"
import { useCover } from "@/lib/useCover"
import { cn } from "@/lib/utils"

/**
 * 2:3 box showing the whole cover, with a blurred copy filling the gaps when the cover's shape
 * differs. Books without a thumbnail get an icon instead.
 */
export function BookCover({ id, className }: { id: string; className?: string }) {
  const { url, failed } = useCover(id)
  return (
    <div
      className={cn("relative aspect-[2/3] overflow-hidden rounded-md bg-muted shadow-sm", className)}
    >
      {failed ? (
        <div className="flex size-full items-center justify-center text-muted-foreground">
          <BookOpen className="size-1/3" />
        </div>
      ) : (
        url && (
          <>
            {/* Scaled up so the blur's soft, see-through edges fall outside the box. */}
            <img
              src={url}
              alt=""
              aria-hidden
              className="absolute inset-0 size-full scale-125 object-cover opacity-70 blur-xl"
            />
            <img src={url} alt="" className="relative size-full object-contain" />
          </>
        )
      )}
    </div>
  )
}
