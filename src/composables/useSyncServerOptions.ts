export const useSyncServerOptions = () => {
  const { t } = useI18n()

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
