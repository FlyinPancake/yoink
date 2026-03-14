import createFetchClient from "openapi-fetch";
import createQueryClient from "openapi-react-query";
import type { paths } from "./types.gen";

export const fetchClient = createFetchClient<paths>({
  baseUrl: "/",
});

export const $api = createQueryClient(fetchClient);
