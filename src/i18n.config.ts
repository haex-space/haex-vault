// i18n.config.ts
import tourDe from './stores/tour.de.json'
import tourEn from './stores/tour.en.json'

export default defineI18nConfig(() => ({
  legacy: false,
  locale: 'de',
  fallbackLocale: 'en',
  globalInjection: true,
  messages: {
    de: { tour: tourDe },
    en: { tour: tourEn },
  },
}))
