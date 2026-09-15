import { cn } from "@/lib/utils"

export function Progress({
  value,
  className,
  tone = "accent",
}: {
  value: number
  className?: string
  tone?: "accent" | "pending" | "conflict" | "confirmed"
}) {
  const bar = {
    accent: "bg-accent",
    pending: "bg-pending",
    conflict: "bg-conflict",
    confirmed: "bg-confirmed",
  }[tone]
  return (
    <div
      role="progressbar"
      aria-valuenow={Math.round(value)}
      aria-valuemin={0}
      aria-valuemax={100}
      className={cn("h-1 w-full overflow-hidden rounded-full bg-surface-3", className)}
    >
      <div className={cn("h-full rounded-full transition-[width] duration-500", bar)} style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  )
}
