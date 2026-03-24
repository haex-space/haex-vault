export default defineAppConfig({
  ui: {
    colors: {
      primary: 'sky',
      secondary: 'fuchsia',
      warning: 'yellow',
      danger: 'red',
    },
    button: {
      defaultVariants: {
        size: 'xl',
      },
    },
    input: {
      defaultVariants: {
        size: 'xl',
      },
    },
    textarea: {
      defaultVariants: {
        size: 'xl',
      },
    },
    select: {
      slots: {
        content: 'ring-1 ring-primary shadow-xl',
        itemLabel: 'text-base',
      },
      defaultVariants: {
        size: 'xl',
      },
    },
    selectMenu: {
      slots: {
        content: 'ring-1 ring-primary shadow-xl',
        itemLabel: 'text-base',
      },
      defaultVariants: {
        size: 'xl',
      },
    },
    card: {
      slots: {
        header: 'p-3 @sm:p-4 sm:p-4',
        body: 'p-3 @sm:p-4 sm:p-4',
        footer: 'p-3 @sm:p-4 sm:p-4',
      },
    },
    checkbox: {
      defaultVariants: {
        size: 'xl',
      },
    },
    radioGroup: {
      defaultVariants: {
        size: 'xl',
      },
    },
    switch: {
      defaultVariants: {
        size: 'xl',
      },
    },
    inputMenu: {
      defaultVariants: {
        size: 'xl',
      },
    },
  },
})
