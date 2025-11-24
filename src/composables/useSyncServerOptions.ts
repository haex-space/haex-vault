export const useSyncServerOptions = () => {
  const { t } = useI18n({
    useScope: 'global',
    messages: {
      de: {
        serverOptions: {
          localhost: 'Lokal (localhost:3002)',
          custom: 'Benutzerdefiniert...',
        },
      },
      en: {
        serverOptions: {
          localhost: 'Local (localhost:3002)',
          custom: 'Custom...',
        },
      },
    },
  })

  const serverOptions = computed<ISyncServerOption[]>(() => [
    {
      label: 'HaexSpace',
      value: 'https://sync.haex.space',
    },
    {
      label: t('serverOptions.localhost'),
      value: 'http://localhost:3002',
    },
    {
      label: t('serverOptions.custom'),
      value: 'custom',
    },
  ])

  return {
    serverOptions,
  }
}
