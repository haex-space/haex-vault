/**
 * The choices of an agent's right: nothing, reading, reading and changing. Every row of the
 * rights list offers the same ones (spec 021 FR-008), "holzi bedienen" and the password manager.
 */
export function useRightLevelOptions() {
  const { t } = useI18n()
  return computed(() =>
    (['none', 'read', 'write'] as const).map((value) => ({
      value,
      label: t(`mcpAgents.level.${value}`),
    })),
  )
}
