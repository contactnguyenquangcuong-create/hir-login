import { ALL_COUNTRIES } from "@/lib/allCountries";
export default function CountryImage({ countryCode }: { countryCode: string }) {
    const countryData = ALL_COUNTRIES.find(ct => ct.value == countryCode);
    if (!countryData) return null;
    return (
        <img src={`/Country/${countryData.title}.svg`} alt={countryData.title} width={24} height={24} className="w-6 h-6" />
    )
}