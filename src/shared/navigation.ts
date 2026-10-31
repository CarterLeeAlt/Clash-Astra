/** 导航元数据独立于页面组件，设置页读取它不会反向加载整个路由树。 */
export const navigation = {
  proxies: { label: "Label-Proxies", link: "/" },
  profiles: { label: "Label-Profiles", link: "/profile" },
  rules: { label: "Label-Rules", link: "/rules" },
  connections: { label: "Label-Connections", link: "/connections" },
  logs: { label: "Label-Logs", link: "/logs" },
  test: { label: "Label-Test", link: "/test" },
  settings: { label: "Label-Settings", link: "/settings" },
} as const;
