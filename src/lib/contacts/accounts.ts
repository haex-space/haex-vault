/**
 * How the account of a phone's address book reads (spec 051): Android names it by a package-style
 * type (`org.telegram.messenger`) and a name that is an e-mail address for most accounts but an
 * internal id for messengers. The window shows the service and, only when it says something to a
 * person, the name.
 */

/** Account types people know by a name. */
const SERVICES: Record<string, string> = {
  'com.google': 'Google',
  'org.telegram.messenger': 'Telegram',
  'com.whatsapp': 'WhatsApp',
  'org.thoughtcrime.securesms': 'Signal',
  'ch.threema.app': 'Threema',
  'com.android.exchange': 'Exchange',
  'com.microsoft.office.outlook.USER_ACCOUNT': 'Outlook',
  'com.microsoft.workaccount': 'Microsoft',
  'com.facebook.auth.login': 'Facebook',
  'com.linkedin.android': 'LinkedIn',
  'com.samsung.android.mobileservice': 'Samsung',
  'org.nextcloud': 'Nextcloud',
  'com.nextcloud.client': 'Nextcloud',
  'bitfire.at.davdroid': 'DAVx⁵',
}

/** The service of an account type: its known name, else the type as it is. */
export function accountService(accountType: string): string {
  return SERVICES[accountType] ?? accountType
}

/**
 * The account as a short line: "Google (mara@example.org)", "Telegram". The name is left out
 * unless it is an e-mail address, or the type is unknown and the name is all there is to go by.
 * `phone` is the line for an account without a type (contacts only on the phone).
 */
export function accountLine(
  accountType: string,
  accountName: string,
  phone: string,
): string {
  if (!accountType) return phone
  const service = accountService(accountType)
  const known = accountType in SERVICES
  const showName = accountName.includes('@') || (!known && accountName !== '')
  return showName ? `${service} (${accountName})` : service
}

/** Splits the origin's account, written `type/name`, at the first slash. */
export function splitOriginAccount(account: string): [string, string] {
  const at = account.indexOf('/')
  return at < 0 ? [account, ''] : [account.slice(0, at), account.slice(at + 1)]
}
