import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"
import type { ComponentProps } from "react"
import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-md font-medium transition-[background-color,border-color,color,box-shadow] duration-100 disabled:pointer-events-none disabled:opacity-45 [&_svg]:pointer-events-none [&_svg]:shrink-0 select-none",
  {
    variants: {
      variant: {
        primary:
          "bg-accent text-on-accent border border-transparent hover:bg-accent-hover shadow-k",
        default:
          "bg-surface text-ink border border-line hover:bg-surface-3 shadow-k",
        ghost: "text-muted hover:bg-surface-3 hover:text-ink border border-transparent",
        subtle: "bg-surface-3 text-ink border border-transparent hover:bg-line",
        danger:
          "bg-surface text-conflict border border-line hover:bg-conflict-soft hover:border-conflict/30",
        link: "text-accent underline-offset-4 hover:underline border border-transparent",
      },
      size: {
        sm: "h-7 px-2.5 text-[12.5px] [&_svg]:size-3.5",
        md: "h-8 px-3 text-[13px] [&_svg]:size-4",
        lg: "h-10 px-5 text-[14px] [&_svg]:size-4",
        icon: "size-8 [&_svg]:size-4",
        "icon-sm": "size-7 [&_svg]:size-3.5",
      },
    },
    defaultVariants: { variant: "default", size: "md" },
  },
)

export function Button({
  className,
  variant,
  size,
  asChild = false,
  ...props
}: ComponentProps<"button"> & VariantProps<typeof buttonVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot : "button"
  return <Comp className={cn(buttonVariants({ variant, size }), className)} {...props} />
}

export { buttonVariants }
