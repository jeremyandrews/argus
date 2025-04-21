for file in `find src -name "*rs" | egrep "entity|vector|db|main|lib|analy"`
do
  echo "----------"
  echo $file
  cat $file
  echo "----------"
  echo
done
