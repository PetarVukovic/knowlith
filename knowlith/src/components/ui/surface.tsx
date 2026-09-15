import type { ComponentProps, ReactNode } from "react"
import { cn } from "@/lib/utils"

/** A plain bordered surface. Not every group of elements deserves one. */
export function Panel({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("rounded-lg border border-line bg-surface shadow-k", className)} {...props} />
}

export function PanelHeader({
  title,
  description,
  actions,
  className,
}: {
  title: ReactNode
  description?: ReactNode
  actions?: ReactNode
  className?: string
}) {
  return (
    <div className={cn("flex items-start justify-between gap-4 border-b border-line px-4 py-3", className)}>
      <div className="min-w-0">
        <div className="text-[13.5px] font-semibold text-ink">{title}</div>
        {description ? <div className="mt-0.5 text-[12.5px] text-muted">{description}</div> : null}
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-1.5">{actions}</div> : null}
    </div>
  )
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-line bg-surface-3 px-1 py-px font-sans text-[10.5px] font-medium text-faint">
      {children}
    </kbd>
  )
}

export function Field({
  label,
  hint,
  children,
  htmlFor,
}: {
  label: string
  hint?: ReactNode
  children: ReactNode
  htmlFor?: string
}) {
  return (
    <div className="grid gap-1.5">
      <label htmlFor={htmlFor} className="text-[12.5px] font-medium text-ink">
        {label}
      </label>
      {children}
      {hint ? <div className="text-[12px] text-muted">{hint}</div> : null}
    </div>
  )
}
