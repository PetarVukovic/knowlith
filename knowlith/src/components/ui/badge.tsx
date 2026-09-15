import { cva, type VariantProps } from "class-variance-authority"
import type { ComponentProps } from "react"
import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-sm border px-1.5 py-px text-[11px] font-medium leading-[17px] whitespace-nowrap",
  {
    variants: {
      tone: {
        neutral: "border-line bg-surface-3 text-muted",
        accent: "border-transparent bg-accent-soft text-accent",
        pending: "border-transparent bg-pending-soft text-pending",
        conflict: "border-transparent bg-conflict-soft text-conflict",
        confirmed: "border-transparent bg-confirmed-soft text-confirmed",
        info: "border-transparent bg-info-soft text-info",
        outline: "border-line text-muted",
      },
    },
    defaultVariants: { tone: "neutral" },
  },
)

export function Badge({ className, tone, ...props }: ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ tone }), className)} {...props} />
}
