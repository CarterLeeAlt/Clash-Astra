import axios, { AxiosInstance } from "axios";
import { getClashInfo } from "@/services/cmds";
import { normalizeControllerHost } from "@/utils/controller";
import { LatestResource } from "@/shared/async/latest-resource";

const client = new LatestResource<AxiosInstance>(async () => {
  const info = await getClashInfo();
  if (!info?.server) throw new Error("Mihomo controller is not configured");
  const server = normalizeControllerHost(info.server);
  if (!server) throw new Error("Mihomo controller address is invalid");
  const secret = info.secret;
  const instance = axios.create({
    baseURL: `http://${server}`,
    headers: secret ? { Authorization: `Bearer ${secret}` } : {},
    timeout: 15000,
  });
  instance.interceptors.response.use((r) => r.data);
  return instance;
});

export const invalidateController = () => client.invalidate();

export const getAxios = (force = false) => {
  if (force) client.invalidate();
  return client.get();
};
