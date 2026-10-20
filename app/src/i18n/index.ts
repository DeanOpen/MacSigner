import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const resources = {
  vi: {
    translation: {
      nav: {
        sign: "Ký tài liệu",
        certificates: "Chứng thư số",
        verify: "Kiểm tra chữ ký",
        services: "Dịch vụ",
        tests: "Kiểm thử",
        settings: "Cài đặt",
      },
    },
  },
  en: {
    translation: {
      nav: {
        sign: "Sign document",
        certificates: "Certificates",
        verify: "Verify",
        services: "Services",
        tests: "Self-test",
        settings: "Settings",
      },
    },
  },
};

i18n.use(initReactI18next).init({
  resources,
  lng: "vi",
  fallbackLng: "vi",
  interpolation: { escapeValue: false },
});

export default i18n;
