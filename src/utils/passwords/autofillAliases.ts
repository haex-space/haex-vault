// Default browser-autofill aliases for standard login fields.
// None of the major password managers (KeePass, Bitwarden, LastPass) export
// these, so importers seed them so freshly imported entries work with the
// browser extension out of the box.
export const DEFAULT_AUTOFILL_ALIASES: Record<string, string[]> = {
  username: ['email', 'login', 'user', 'e-mail', 'mail'],
  password: ['pass', 'pwd', 'secret'],
  otpSecret: ['otp', 'totp', '2fa', 'code', 'token'],
}
