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
        size: 'lg',
      },
    },
    input: {
      defaultVariants: {
        size: 'lg',
      },
    },
    textarea: {
      defaultVariants: {
        size: 'lg',
      },
    },
    select: {
      slots: {
        content: 'ring-1 ring-primary shadow-xl',
        itemLabel: 'text-base',
      },
      defaultVariants: {
        size: 'lg',
      },
    },
    selectMenu: {
      slots: {
        content: 'ring-1 ring-primary shadow-xl',
        itemLabel: 'text-base',
      },
      defaultVariants: {
        size: 'lg',
      },
    },
    checkbox: {
      defaultVariants: {
        size: 'lg',
      },
    },
    radioGroup: {
      defaultVariants: {
        size: 'lg',
      },
    },
    switch: {
      defaultVariants: {
        size: 'lg',
      },
    },
    inputMenu: {
      defaultVariants: {
        size: 'lg',
      },
    },
  },
})
