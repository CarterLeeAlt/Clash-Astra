import LogsPage from "@/features/logs/LogsPage";
import ProxiesPage from "@/features/proxies/ProxiesPage";
import TestPage from "@/features/test/TestPage";
import ProfilesPage from "@/features/profiles/ProfilesPage";
import SettingsPage from "@/features/settings/SettingsPage";
import ConnectionsPage from "@/features/connections/ConnectionsPage";
import RulesPage from "@/features/rules/RulesPage";
import { navigation } from "@/shared/navigation";

import ProxiesSvg from "@/assets/image/itemicon/proxies.svg?react";
import ProfilesSvg from "@/assets/image/itemicon/profiles.svg?react";
import ConnectionsSvg from "@/assets/image/itemicon/connections.svg?react";
import RulesSvg from "@/assets/image/itemicon/rules.svg?react";
import LogsSvg from "@/assets/image/itemicon/logs.svg?react";
import TestSvg from "@/assets/image/itemicon/test.svg?react";
import SettingsSvg from "@/assets/image/itemicon/settings.svg?react";

export const routers = [
  {
    ...navigation.proxies,
    icon: <ProxiesSvg />,
    ele: ProxiesPage,
  },
  {
    ...navigation.profiles,
    icon: <ProfilesSvg />,
    ele: ProfilesPage,
  },
  {
    ...navigation.rules,
    icon: <RulesSvg />,
    ele: RulesPage,
  },
  {
    ...navigation.connections,
    icon: <ConnectionsSvg />,
    ele: ConnectionsPage,
  },
  {
    ...navigation.logs,
    icon: <LogsSvg />,
    ele: LogsPage,
  },
  {
    ...navigation.test,
    icon: <TestSvg />,
    ele: TestPage,
  },
  {
    ...navigation.settings,
    icon: <SettingsSvg />,
    ele: SettingsPage,
  },
];
