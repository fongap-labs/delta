import type { SidebarBrandMarkOwnerProps } from '@deepseek-ai/dsh-client-ui-sidebar/client'

const logoUrl = new URL('./assets/delta-logo-256x256.png', import.meta.url).href

/** Delta's legacy mark, without changing the Harness plugin slot contract. */
export function OfficialBrandMark({ size }: SidebarBrandMarkOwnerProps) {
  return <img src={logoUrl} alt="Delta" width={size} height={size} style={{ objectFit: 'contain' }} />
}

/** Delta product name for the existing brand-name slot. */
export function OfficialBrandName() {
  return <span style={{ color: 'inherit', fontWeight: 700, fontSize: 20, letterSpacing: '.02em' }}>Delta</span>
}
