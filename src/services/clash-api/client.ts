import axios, { AxiosInstance } from "axios";
import { getClashInfo } from "@/services/cmds";
import { normalizeControllerHost } from "@/utils/controller";

let axiosIns: AxiosInstance = null!;

/// initialize some information
/// enable force update axiosIns
export const getAxios = async (force: boolean = false) => {
  if (axiosIns && !force) return axiosIns;

  let server = "";
  let secret = "";

  try {
    const info = await getClashInfo();

    if (info?.server) {
      server = normalizeControllerHost(info.server);
    }
    if (info?.secret) secret = info?.secret;
  } catch {}

  axiosIns = axios.create({
    baseURL: `http://${server}`,
    headers: secret ? { Authorization: `Bearer ${secret}` } : {},
    timeout: 15000,
  });
  axiosIns.interceptors.response.use((r) => r.data);
  return axiosIns;
};
