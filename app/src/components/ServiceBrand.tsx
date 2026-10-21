import esignerLogo from "../assets/services/esigner.png";
import ctsigninghubLogo from "../assets/services/ctsigninghub.png";
import vnptLogo from "../assets/brands/vnpt-ca.jpg";

export const serviceBrands = {
  esign: { name: "eSigner", version: "1.1.0", logo: esignerLogo },
  ctsign: { name: "CTSigningHub", version: "1.2.0", logo: ctsigninghubLogo },
  vnpt: { name: "VNPT-CA Plugin", version: "Thử nghiệm", logo: vnptLogo },
} as const;

export function ServiceLogo({ service, className }: { service: keyof typeof serviceBrands; className?: string }) {
  const brand = serviceBrands[service];
  return <img src={brand.logo} alt={`Logo ${brand.name}`} className={className} draggable={false} />;
}
