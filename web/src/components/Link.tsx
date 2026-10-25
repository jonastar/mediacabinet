import type { AnchorHTMLAttributes, MouseEvent } from 'react'
import { navigate } from '../lib/router'

type LinkProps = AnchorHTMLAttributes<HTMLAnchorElement> & { to: string }

/** An anchor that navigates without reloading; modified clicks still open new tabs. */
export function Link({ to, onClick, ...props }: LinkProps) {
  const handle = (e: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(e)
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) {
      return
    }
    e.preventDefault()
    navigate(to)
  }
  return <a href={to} onClick={handle} {...props} />
}
